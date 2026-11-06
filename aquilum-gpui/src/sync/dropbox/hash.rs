use crate::sync::remote::Content;
use std::io;

pub fn content_hash(content: &dyn Content) -> io::Result<String> {
    crate::sync::hash::dropbox_hash(content)
}
