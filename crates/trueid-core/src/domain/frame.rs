#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamModality {
    Rgb,
    Ir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Rgb8,
    Gray8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub modality: StreamModality,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct FramePair {
    pub rgb: Frame,
    pub ir: Option<Frame>,
}
