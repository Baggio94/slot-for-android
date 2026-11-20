//! Favorites by each game's portable name (`library::game_key`, slot. 2.5),
//! so they follow the games to another handheld. Entries from 2.4, by
//! Android address, still match and take the name when next saved.
use slot_store::Cart;
use std::path::Path;
const FILE: &str = "konkr-favorites.json";

pub fn load(root: &Path) -> Vec<String> {
    let entries = std::fs::read(root.join(FILE)).ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
        .unwrap_or_default();
    let mut unique = Vec::new();
    for uri in entries {
        if !uri.is_empty() && !unique.contains(&uri) { unique.push(uri); }
    }
    unique
}
pub fn toggle(root: &Path, uri: &str) -> std::io::Result<bool> {
    if uri.is_empty() { return Ok(false); }
    let name = crate::library::game_key(uri);
    let mut list = named(load(root));
    let added = if list.iter().any(|s| *s == name || s == uri) {
        list.retain(|item| *item != name && item != uri);
        false
    } else { list.push(name); true };
    store(root,&list)?;
    Ok(added)
}

/// 2.4's entries by Android address, by name where the library knows them.
pub fn named(list: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(list.len());
    for item in list {
        let name = if item.starts_with("content://") { crate::library::game_key(&item) } else { item };
        if !out.contains(&name) { out.push(name); }
    }
    out
}

/// Whether this cart is in the list, by name or by its 2.4 address.
pub fn holds(list: &[String], uri: &str) -> bool {
    list.iter().any(|item| item == uri) || {
        let name = crate::library::game_key(uri);
        list.iter().any(|item| *item == name)
    }
}
fn store(root: &Path, list: &[String]) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    let bytes=serde_json::to_vec_pretty(list).map_err(std::io::Error::other)?;
    slot_store::atomic_write(&root.join(FILE),&bytes)
}
/// Every cart by its URI, the first one when two share it: found at once
/// rather than by walking every shelf for each URI (thousands of carts).
pub fn by_uri<'a>(sources: &[&'a [Cart]]) -> std::collections::HashMap<String, &'a Cart> {
    let keys = crate::library::LIBRARY.lock().unwrap_or_else(|e| e.into_inner()).keys.clone();
    let mut carts = std::collections::HashMap::new();
    for cart in sources.iter().flat_map(|items| items.iter()) {
        let uri = cart.rom.to_string_lossy().into_owned();
        // By portable name too (2.5): lists saved on another handheld.
        if let Some(name) = keys.get(&uri) { carts.entry(name.clone()).or_insert(cart); }
        carts.entry(uri).or_insert(cart);
    }
    carts
}

pub fn carts(ids: &[String], sources: &[&[Cart]]) -> Vec<Cart> {
    let carts = by_uri(sources);
    let mut rows: Vec<Cart> = ids.iter()
        .filter_map(|uri| carts.get(uri.as_str()).map(|cart| (*cart).clone()))
        .collect();
    rows.sort_by(|a,b| {
        (slot_store::sort_key(&a.stem),a.platform as u8)
            .cmp(&(slot_store::sort_key(&b.stem),b.platform as u8))
    });
    rows
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn toggle_is_durable_reversible_and_deduplicated() {
        let dir=std::env::temp_dir().join(format!("slot-favorites-u2-{}",std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(toggle(&dir,"content://roms/emerald").unwrap());
        assert_eq!(load(&dir),vec!["content://roms/emerald"]);
        assert!(!toggle(&dir,"content://roms/emerald").unwrap());
        assert!(load(&dir).is_empty());
        store(&dir,&["content://roms/a".into(),"content://roms/a".into()]).unwrap();
        assert_eq!(load(&dir),vec!["content://roms/a"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}