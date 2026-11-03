use super::*;

    pub(super) fn scratch_doc() -> crate::document_buffer::DocumentBuffer {
        crate::document_buffer::DocumentBuffer::new(
            DocumentId::new(),
            std::path::PathBuf::new(),
            String::new(),
            String::new(),
            0,
        )
    }
