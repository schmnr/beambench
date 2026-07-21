//! Barcode types for 1D and 2D barcode generation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarcodeType {
    Code128,
    Code39,
    Code93,
    Codabar,
    #[serde(rename = "standard_2_of_5")]
    Standard2Of5,
    Ean13,
    Ean8,
    UpcA,
    QrCode,
    DataMatrix,
    Pdf417,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QrErrorCorrection {
    Low,
    #[default]
    Medium,
    Quartile,
    High,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarcodeOptions {
    #[serde(default)]
    pub show_text: bool,
    #[serde(default)]
    pub qr_error_correction: QrErrorCorrection,
    #[serde(default)]
    pub data_matrix_force_square: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn barcode_type_code128_roundtrips() {
        let barcode = BarcodeType::Code128;
        let json = serde_json::to_string(&barcode).unwrap();
        let restored: BarcodeType = serde_json::from_str(&json).unwrap();
        assert_eq!(barcode, restored);
        assert_eq!(json, "\"code128\"");
    }

    #[test]
    fn barcode_type_code39_roundtrips() {
        let barcode = BarcodeType::Code39;
        let json = serde_json::to_string(&barcode).unwrap();
        let restored: BarcodeType = serde_json::from_str(&json).unwrap();
        assert_eq!(barcode, restored);
        assert_eq!(json, "\"code39\"");
    }

    #[test]
    fn barcode_type_code93_roundtrips() {
        let barcode = BarcodeType::Code93;
        let json = serde_json::to_string(&barcode).unwrap();
        let restored: BarcodeType = serde_json::from_str(&json).unwrap();
        assert_eq!(barcode, restored);
        assert_eq!(json, "\"code93\"");
    }

    #[test]
    fn barcode_type_codabar_roundtrips() {
        let barcode = BarcodeType::Codabar;
        let json = serde_json::to_string(&barcode).unwrap();
        let restored: BarcodeType = serde_json::from_str(&json).unwrap();
        assert_eq!(barcode, restored);
        assert_eq!(json, "\"codabar\"");
    }

    #[test]
    fn barcode_type_standard_2_of_5_roundtrips() {
        let barcode = BarcodeType::Standard2Of5;
        let json = serde_json::to_string(&barcode).unwrap();
        let restored: BarcodeType = serde_json::from_str(&json).unwrap();
        assert_eq!(barcode, restored);
        assert_eq!(json, "\"standard_2_of_5\"");
    }

    #[test]
    fn barcode_type_ean13_roundtrips() {
        let barcode = BarcodeType::Ean13;
        let json = serde_json::to_string(&barcode).unwrap();
        let restored: BarcodeType = serde_json::from_str(&json).unwrap();
        assert_eq!(barcode, restored);
        assert_eq!(json, "\"ean13\"");
    }

    #[test]
    fn barcode_type_ean8_roundtrips() {
        let barcode = BarcodeType::Ean8;
        let json = serde_json::to_string(&barcode).unwrap();
        let restored: BarcodeType = serde_json::from_str(&json).unwrap();
        assert_eq!(barcode, restored);
        assert_eq!(json, "\"ean8\"");
    }

    #[test]
    fn barcode_type_upca_roundtrips() {
        let barcode = BarcodeType::UpcA;
        let json = serde_json::to_string(&barcode).unwrap();
        let restored: BarcodeType = serde_json::from_str(&json).unwrap();
        assert_eq!(barcode, restored);
        assert_eq!(json, "\"upc_a\"");
    }

    #[test]
    fn barcode_type_qr_code_roundtrips() {
        let barcode = BarcodeType::QrCode;
        let json = serde_json::to_string(&barcode).unwrap();
        let restored: BarcodeType = serde_json::from_str(&json).unwrap();
        assert_eq!(barcode, restored);
        assert_eq!(json, "\"qr_code\"");
    }

    #[test]
    fn barcode_type_data_matrix_roundtrips() {
        let barcode = BarcodeType::DataMatrix;
        let json = serde_json::to_string(&barcode).unwrap();
        let restored: BarcodeType = serde_json::from_str(&json).unwrap();
        assert_eq!(barcode, restored);
        assert_eq!(json, "\"data_matrix\"");
    }

    #[test]
    fn barcode_type_pdf417_roundtrips() {
        let barcode = BarcodeType::Pdf417;
        let json = serde_json::to_string(&barcode).unwrap();
        let restored: BarcodeType = serde_json::from_str(&json).unwrap();
        assert_eq!(barcode, restored);
        assert_eq!(json, "\"pdf417\"");
    }
}
