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

pub fn build_started(operation: &OperationContext) {
    operation.emit(Event::BuildStarted);
}

pub fn section_started(operation: &OperationContext, title: impl Into<String>) {
    operation.emit(Event::SectionStarted {
        title: title.into(),
    });
}

pub fn success(operation: &OperationContext, message: impl Into<String>) {
    operation.emit(Event::Success {
        message: message.into(),
    });
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
