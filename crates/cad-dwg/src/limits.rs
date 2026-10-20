//! Bounds on what a read may consume (DESIGN §43).

/// Every quantity a DWG file can grow is bounded here. Reaching a bound is
/// a structured error, never an allocation in proportion to a claimed
/// count: sections and pages are decoded into buffers that grow with the
/// bytes produced, and an object is read within the bytes it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportLimits {
    /// Bytes of file.
    pub max_bytes: usize,
    /// Sections a section map may describe.
    pub max_sections: usize,
    /// Pages a page map may describe, and pages one section may have.
    pub max_pages: usize,
    /// Bytes of one decoded section: the objects, the handles, the classes,
    /// the header.
    pub max_section_bytes: usize,
    /// Objects the object map may list.
    pub max_objects: usize,
    /// Bytes of one object.
    pub max_object_bytes: usize,
    /// Bytes of one string.
    pub max_string_bytes: usize,
    /// Classes the classes section may define.
    pub max_classes: usize,
}

impl Default for ImportLimits {
    fn default() -> Self {
        Self {
            max_bytes: 256 * 1024 * 1024,
            max_sections: 64,
            max_pages: 1 << 16,
            max_section_bytes: 256 * 1024 * 1024,
            max_objects: 4_000_000,
            max_object_bytes: 16 * 1024 * 1024,
            max_string_bytes: 64 * 1024,
            max_classes: 10_000,
        }
    }
}
