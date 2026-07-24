//! Event helpers shared by build backends and runners.

use crate::event::{DiagnosticLevel, Event};
use crate::operation::OperationContext;

pub fn console_print(operation: &OperationContext, text: &str) {
    operation.emit(Event::Diagnostic {
        level: DiagnosticLevel::Info,
        message: text.to_owned(),
    });
}

pub fn build_step(operation: &OperationContext, description: impl Into<String>) {
    operation.emit(Event::BuildStep {
        description: description.into(),
    });
}

pub fn rule(operation: &OperationContext) {
    console_print(operation, &"-".repeat(80));
}

pub fn print_panel(operation: &OperationContext, content: &str) {
    operation.emit(Event::Content {
        content: content.to_owned(),
        language: None,
    });
}

pub fn print_syntax_panel(operation: &OperationContext, content: &str, language: &str) {
    operation.emit(Event::Content {
        content: content.to_owned(),
        language: Some(language.to_owned()),
    });
}
