//! The colour space and chroma siting a pixel buffer's attachments state.

use objc2_core_foundation::{CFRetained, CFString};
use objc2_core_video::{
    CVPixelBuffer, kCVImageBufferChromaLocation_Center, kCVImageBufferChromaLocation_TopLeft,
    kCVImageBufferChromaLocationTopFieldKey, kCVImageBufferColorPrimaries_EBU_3213,
    kCVImageBufferColorPrimaries_ITU_R_2020, kCVImageBufferColorPrimaries_P3_D65,
    kCVImageBufferColorPrimaries_SMPTE_C, kCVImageBufferColorPrimariesKey,
    kCVImageBufferTransferFunction_ITU_R_2100_HLG, kCVImageBufferTransferFunction_Linear,
    kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ, kCVImageBufferTransferFunction_sRGB,
    kCVImageBufferTransferFunctionKey, kCVImageBufferYCbCrMatrix_ITU_R_601_4,
    kCVImageBufferYCbCrMatrix_ITU_R_2020, kCVImageBufferYCbCrMatrix_SMPTE_240M_1995,
    kCVImageBufferYCbCrMatrixKey,
};
use zgui_wgpu::{ChromaSiting, ColorMatrix, ColorPrimaries, ColorSpace, TransferFunction};

/// The string attached to `buffer` under `key`, if there is one.
fn attachment(buffer: &CVPixelBuffer, key: &CFString) -> Option<CFRetained<CFString>> {
    // SAFETY: a null mode pointer is permitted; the mode is not wanted.
    let value = unsafe { buffer.attachment(key, std::ptr::null_mut()) }?;
    value.downcast::<CFString>().ok()
}

/// Whether `value` is `expected`.
fn is(value: &CFString, expected: &CFString) -> bool {
    value == expected
}

/// The colour space `buffer` states. An absent or unknown attachment reads as its BT.709 value.
pub(super) fn color_space(buffer: &CVPixelBuffer) -> ColorSpace {
    let mut space = ColorSpace::BT709;
    // SAFETY: the keys and values are CoreVideo's own constant strings, valid for the process.
    unsafe {
        if let Some(matrix) = attachment(buffer, kCVImageBufferYCbCrMatrixKey) {
            space.matrix = if is(&matrix, kCVImageBufferYCbCrMatrix_ITU_R_601_4) {
                ColorMatrix::Bt601
            } else if is(&matrix, kCVImageBufferYCbCrMatrix_ITU_R_2020) {
                ColorMatrix::Bt2020
            } else if is(&matrix, kCVImageBufferYCbCrMatrix_SMPTE_240M_1995) {
                ColorMatrix::Smpte240
            } else {
                ColorMatrix::Bt709
            };
        }
        if let Some(primaries) = attachment(buffer, kCVImageBufferColorPrimariesKey) {
            space.primaries = if is(&primaries, kCVImageBufferColorPrimaries_ITU_R_2020) {
                ColorPrimaries::Bt2020
            } else if is(&primaries, kCVImageBufferColorPrimaries_P3_D65) {
                ColorPrimaries::DisplayP3
            } else if is(&primaries, kCVImageBufferColorPrimaries_SMPTE_C) {
                ColorPrimaries::Bt601
            } else if is(&primaries, kCVImageBufferColorPrimaries_EBU_3213) {
                ColorPrimaries::Bt470Bg
            } else {
                ColorPrimaries::Bt709
            };
        }
        if let Some(transfer) = attachment(buffer, kCVImageBufferTransferFunctionKey) {
            space.transfer = if is(&transfer, kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ) {
                TransferFunction::Pq
            } else if is(&transfer, kCVImageBufferTransferFunction_ITU_R_2100_HLG) {
                TransferFunction::Hlg
            } else if is(&transfer, kCVImageBufferTransferFunction_sRGB) {
                TransferFunction::Srgb
            } else if is(&transfer, kCVImageBufferTransferFunction_Linear) {
                TransferFunction::Linear
            } else {
                TransferFunction::Bt709
            };
        }
    }
    space
}

/// Where `buffer` states its chroma samples sit. An absent attachment reads as left.
pub(super) fn chroma_siting(buffer: &CVPixelBuffer) -> ChromaSiting {
    // SAFETY: the key and values are CoreVideo's own constant strings, valid for the process.
    unsafe {
        match attachment(buffer, kCVImageBufferChromaLocationTopFieldKey) {
            Some(location) if is(&location, kCVImageBufferChromaLocation_Center) => {
                ChromaSiting::Center
            }
            Some(location) if is(&location, kCVImageBufferChromaLocation_TopLeft) => {
                ChromaSiting::TopLeft
            }
            _ => ChromaSiting::Left,
        }
    }
}
