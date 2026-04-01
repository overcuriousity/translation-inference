pub mod docx;
pub mod odt;
pub mod pdf;

pub use docx::translate_docx;
pub use odt::translate_odt;
pub use pdf::translate_pdf;
