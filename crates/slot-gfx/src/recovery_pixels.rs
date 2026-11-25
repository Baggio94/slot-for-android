//! Lossless CPU copies of selected static sprites. GPU pixels and texture
//! ids are unchanged. Compression is best-effort on a single low-priority
//! worker; context loss can always use the raw copy until packing finishes.
use std::sync::{Arc, Mutex, OnceLock, Weak, mpsc};

const MIN_BYTES: usize = 256 * 1024;
const QUEUE: usize = 128;

#[derive(Clone)]
enum Stored {
    Raw(Arc<[u8]>),
    Packed(Arc<[u8]>),
}

pub(crate) struct RecoveryPixels(Mutex<Stored>);

impl RecoveryPixels {
    pub(crate) fn new(rgba: &[u8]) -> Arc<Self> {
        Arc::new(Self(Mutex::new(Stored::Raw(rgba.into()))))
    }

    pub(crate) fn len(&self) -> usize {
        match &*self.0.lock().unwrap_or_else(|e| e.into_inner()) {
            Stored::Raw(bytes) | Stored::Packed(bytes) => bytes.len(),
        }
    }

    pub(crate) fn with_rgba<T>(&self, f: impl FnOnce(&[u8]) -> T) -> Result<T, String> {
        let stored = self.0.lock().unwrap_or_else(|e| e.into_inner()).clone();
        match stored {
            Stored::Raw(bytes) => Ok(f(&bytes)),
            Stored::Packed(bytes) => {
                let rgba = lz4_flex::decompress_size_prepended(&bytes)
                    .map_err(|e| format!("sprite recovery: {e}"))?;
                Ok(f(&rgba))
            }
        }
    }

    pub(crate) fn schedule(this: &Arc<Self>) {
        if this.len() < MIN_BYTES { return; }
        static WORKER: OnceLock<Option<mpsc::SyncSender<Weak<RecoveryPixels>>>> = OnceLock::new();
        let sender = WORKER.get_or_init(|| {
            let (tx, rx) = mpsc::sync_channel::<Weak<RecoveryPixels>>(QUEUE);
            std::thread::Builder::new().name("slot-texture-pack".into())
                .spawn(move || {
                    #[cfg(target_os = "android")]
                    unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, 15); }
                    while let Ok(job) = rx.recv() {
                        if let Some(pixels) = job.upgrade() { pixels.pack(); }
                    }
                }).ok().map(|_| tx)
        });
        // Never stall a texture upload. A full queue simply keeps this raw.
        if let Some(sender) = sender { let _ = sender.try_send(Arc::downgrade(this)); }
    }

    fn pack(&self) {
        let raw = match &*self.0.lock().unwrap_or_else(|e| e.into_inner()) {
            Stored::Raw(bytes) if bytes.len() >= MIN_BYTES => Arc::clone(bytes),
            _ => return,
        };
        // No lock is held while compressing or allocating the packed buffer.
        let packed = lz4_flex::compress_prepend_size(&raw);
        if packed.len() > raw.len() * 3 / 4 { return; }
        let packed: Arc<[u8]> = packed.into();
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Stored::Packed(packed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lossless_static_pixels_and_small_copies() {
        let raw = [12, 34, 56, 255].repeat(256 * 256);
        let pixels = RecoveryPixels::new(&raw);
        assert_eq!(pixels.len(), raw.len());
        pixels.pack();
        assert!(pixels.len() < raw.len() / 4);
        assert!(pixels.with_rgba(|bytes| bytes == raw.as_slice()).unwrap());
        let small = RecoveryPixels::new(&[1, 2, 3, 4]);
        small.pack();
        assert_eq!(small.len(), 4);
        assert_eq!(small.with_rgba(|bytes| bytes.to_vec()).unwrap(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn incompressible_pixels_remain_raw() {
        let mut x = 0x12345678_u32;
        let raw: Vec<u8> = (0..MIN_BYTES).map(|_| {
            x ^= x << 13; x ^= x >> 17; x ^= x << 5; x as u8
        }).collect();
        let pixels = RecoveryPixels::new(&raw);
        pixels.pack();
        assert_eq!(pixels.len(), raw.len());
        assert!(pixels.with_rgba(|bytes| bytes == raw.as_slice()).unwrap());
    }

    #[test]
    fn an_updated_texture_owns_an_independent_recovery_copy() {
        let old = RecoveryPixels::new(&vec![7; MIN_BYTES]);
        let current = RecoveryPixels::new(&vec![8; MIN_BYTES]);
        old.pack();
        assert!(current.with_rgba(|bytes| bytes.iter().all(|b| *b == 8)).unwrap());
    }
}
