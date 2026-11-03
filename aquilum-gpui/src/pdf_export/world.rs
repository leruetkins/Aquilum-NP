use std::collections::HashMap;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

/// Typst World implementation for in-memory document compilation.
pub struct MemoryWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    source: Source,
    main_id: FileId,
    pub(crate) files: HashMap<FileId, Bytes>,
    now: Datetime,
}

impl MemoryWorld {
    pub fn new(source_text: String) -> Self {
        let vpath = VirtualPath::new("main.typ").expect("valid main.typ path");
        let rooted_path = RootedPath::new(VirtualRoot::Project, vpath);
        let main_id = FileId::new(rooted_path);
        let source = Source::new(main_id, source_text);

        let mut book = FontBook::new();
        let mut fonts = Vec::new();

        // Load embedded fonts from typst-assets
        for font_bytes in typst_assets::fonts() {
            let buffer = Bytes::new(font_bytes);
            for font in Font::iter(buffer) {
                book.push(font.info().clone());
                fonts.push(font);
            }
        }

        // Also load embedded fonts from app assets if available
        let app_fonts: [&[u8]; 6] = [
            include_bytes!("../../assets/fonts/roboto_mono_regular.ttf"),
            include_bytes!("../../assets/fonts/roboto_mono_bold.ttf"),
            include_bytes!("../../assets/fonts/roboto_mono_italic.ttf"),
            include_bytes!("../../assets/fonts/roboto_mono_bold_italic.ttf"),
            include_bytes!("../../assets/fonts/droid_sans_mono.ttf"),
            include_bytes!("../../assets/fonts/droid_sans_mono_bold.ttf"),
        ];
        for font_bytes in app_fonts {
            let buffer = Bytes::new(font_bytes);
            for font in Font::iter(buffer) {
                book.push(font.info().clone());
                fonts.push(font);
            }
        }

        let library = LazyHash::new(Library::default());
        let book = LazyHash::new(book);

        Self {
            library,
            book,
            fonts,
            source,
            main_id,
            files: HashMap::new(),
            now: Datetime::from_ymd(2026, 1, 1).unwrap(),
        }
    }
}

impl World for MemoryWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.main_id
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main_id {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if let Some(bytes) = self.files.get(&id) {
            Ok(bytes.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        Some(self.now)
    }
}
