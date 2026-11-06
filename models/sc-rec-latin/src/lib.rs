//! The model package `sc-rec-latin` compiled in (see its MODEL.md). The same folder loads at runtime with
//! `ModelPackage::load`.

use sc_ohseearr_core::ModelPackage;

/// The package: the model, its manifest and its dictionary.
pub fn package() -> ModelPackage {
    ModelPackage::embedded(
        include_str!("../model.toml"),
        include_bytes!("../model.onnx"),
        Some(include_str!("../dict.txt")),
    )
    .expect("the embedded package is valid")
}

#[cfg(test)]
mod tests {
    #[test]
    fn package_is_valid() {
        let p = super::package();
        assert_eq!(p.manifest.id, "sc-rec-latin");
    }
}
