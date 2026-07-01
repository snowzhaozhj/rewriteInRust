//! 语言适配器工厂。

use crate::error::{MigrateError, Result};
use crate::lang::go::GoAdapter;
use crate::lang::python::PythonAdapter;
use crate::lang::typescript::TypeScriptAdapter;
use crate::lang::LanguageAdapter;
use crate::types::common::SourceLang;

/// 根据源语言创建对应的适配器实例。
pub fn create_adapter(lang: SourceLang) -> Result<Box<dyn LanguageAdapter>> {
    match lang {
        SourceLang::TypeScript => Ok(Box::new(TypeScriptAdapter::new()?)),
        SourceLang::Python => Ok(Box::new(PythonAdapter::new()?)),
        SourceLang::Go => Ok(Box::new(GoAdapter::new()?)),
        _ => Err(MigrateError::NotImplemented(format!(
            "语言适配器尚未实现: {lang}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_typescript_adapter() {
        let adapter = create_adapter(SourceLang::TypeScript).unwrap();
        assert_eq!(adapter.language(), SourceLang::TypeScript);
    }

    #[test]
    fn create_python_adapter() {
        let adapter = create_adapter(SourceLang::Python).unwrap();
        assert_eq!(adapter.language(), SourceLang::Python);
    }

    #[test]
    fn create_go_adapter() {
        let adapter = create_adapter(SourceLang::Go).unwrap();
        assert_eq!(adapter.language(), SourceLang::Go);
        assert!(adapter.can_handle(std::path::Path::new("main.go")));
        assert!(!adapter.can_handle(std::path::Path::new("main.py")));
    }

    #[test]
    fn unsupported_language_returns_error() {
        let result = create_adapter(SourceLang::C);
        assert!(result.is_err());
        let msg = result.as_ref().err().unwrap().to_string();
        assert!(msg.contains("尚未实现"));
    }
}
