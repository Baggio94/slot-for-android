//! Durable last selected shelf and ROM, without touching game/save data.
use serde::{Serialize,Deserialize};
use std::path::Path;
const FILE: &str="konkr-shelf-focus.json";
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
pub struct Focus { pub rack: usize, pub uri: String }
/// As written: how many system shelves came before the special racks
/// (6 until 3.0, when the file had no such field).
#[derive(Serialize,Deserialize)]
struct Stored { rack: usize, uri: String, #[serde(default="six")] systems: usize }
fn six() -> usize { 6 }
pub fn load(root: &Path) -> Option<Focus> {
    let systems=slot_store::Platform::ALL.len();
    std::fs::read(root.join(FILE)).ok()
        .and_then(|bytes|serde_json::from_slice::<Stored>(&bytes).ok())
        // Recents, Favorites and GBA Video follow the system shelves: a
        // focus written with fewer systems moves along with them.
        .map(|s| Focus {
            rack: if s.rack>=s.systems { s.rack+systems.saturating_sub(s.systems) } else { s.rack },
            uri: s.uri,
        })
        .filter(|focus|focus.rack<systems+3 && !focus.uri.is_empty())
}
pub fn store(root: &Path, focus: &Focus) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    let temp=root.join(format!("{FILE}.partial"));
    let stored=Stored { rack: focus.rack, uri: focus.uri.clone(), systems: slot_store::Platform::ALL.len() };
    let data=serde_json::to_vec(&stored).map_err(std::io::Error::other)?;
    std::fs::write(&temp,data)?;
    let final_path=root.join(FILE);
    match std::fs::rename(&temp,&final_path) {
        Ok(())=>Ok(()),
        Err(e)=>{let _=std::fs::remove_file(&temp);Err(e)}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn persist_restore_and_ignore_missing_state() {
        let dir=std::env::temp_dir().join(format!("slot-focus-u2-{}",std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(load(&dir),None);
        let focus=Focus { rack: 7,uri:"content://roms/emerald".into() };
        store(&dir,&focus).unwrap();
        assert_eq!(load(&dir),Some(focus));
        let racks = slot_store::Platform::ALL.len() + 3;
        store(&dir, &Focus { rack: racks, uri: "content://roms/bad".into() }).unwrap();
        assert_eq!(load(&dir), None, "out-of-range rack must not restore");
        // 2.5's file (six systems): its Favorites rack (7) is Favorites still.
        std::fs::write(dir.join(FILE), r#"{"rack":7,"uri":"content://roms/fav"}"#).unwrap();
        assert_eq!(load(&dir).map(|f| f.rack), Some(slot_store::Platform::ALL.len() + 1));
        std::fs::write(dir.join(FILE), r#"{"rack":5,"uri":"content://roms/md"}"#).unwrap();
        assert_eq!(load(&dir).map(|f| f.rack), Some(5));
        std::fs::remove_dir_all(dir).unwrap();
    }
}