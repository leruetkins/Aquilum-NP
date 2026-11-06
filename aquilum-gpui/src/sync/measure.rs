use crate::sync::cycle::{Vault, LARGE};
use crate::sync::records::{Large, Record, Size};
use crate::sync::scan::{scan, service, LocalFile};
use std::collections::{HashMap, HashSet};
use std::io;

pub struct Local {
    pub files: HashMap<String, LocalFile>,
    pub kept: HashSet<String>,
}

pub fn survey(vault: &Vault<'_>, records: &HashMap<String, Record>) -> io::Result<Local> {
    let (mut local, mut size) = (
        Local { files: HashMap::new(), kept: HashSet::new() },
        Size::default(),
    );
    let mut large = Vec::new();
    for (path, file) in scan(vault.root, &vault.scope)? {
        if vault.ignored.contains(&path) {
            // The user paused this path: leave it on disk and out of every
            // transfer or deletion decision. Keep it visible so it can be
            // returned to sync later.
            large.push(Large {
                path: path.clone(),
                size: file.size,
                kept: false,
                ignored: true,
            });
            local.kept.insert(path);
            continue;
        }
        let fits = file.size <= LARGE
            || vault.allowed.contains(&path)
            || records.get(&path).is_some_and(|record| record.remote.size > LARGE);
        if file.size > LARGE && !service(&path) {
            large.push(Large { path: path.clone(), size: file.size, kept: !fits, ignored: false });
        }
        if fits {
            size.synced += file.size;
            size.files += 1;
            local.files.insert(path, file);
        } else {
            local.kept.insert(path);
        }
    }
    large.sort_by(|left: &Large, right: &Large| right.size.cmp(&left.size));
    vault.watch.measured(large, size);
    Ok(local)
}
