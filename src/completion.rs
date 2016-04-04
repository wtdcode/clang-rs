// Copyright 2016 Kyle Mayes
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Code completion.

use std::fmt;
use std::mem;
use std::ptr;
use std::slice;
use std::cmp::{self, Ordering};
use std::marker::{PhantomData};
use std::path::{PathBuf};

use clang_sys as ffi;

use libc::{c_uint};

use utility;
use super::{Availability, EntityKind, TranslationUnit, Unsaved, Usr};
use super::diagnostic::{Diagnostic};

//================================================
// Enums
//================================================

// CompletionChunk _______________________________

/// A piece of a code completion string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompletionChunk<'r> {
    /// A colon (`':'`).
    Colon,
    /// A comma (`','`).
    Comma,
    /// An equals sign (`'='`).
    Equals,
    /// A semicolon (`';'`).
    Semicolon,
    /// A left angle bracket (`'<'`).
    LeftAngleBracket,
    /// A right angle bracket (`'>'`).
    RightAngleBracket,
    /// A left brace (`'{'`).
    LeftBrace,
    /// A right brace (`'}'`).
    RightBrace,
    /// A left parentesis (`'('`)).
    LeftParenthesis,
    /// A right parenthesis (`')'`).
    RightParenthesis,
    /// A left square bracket (`'['`).
    LeftSquareBracket,
    /// A right square bracket (`']'`).
    RightSquareBracket,
    /// Horizontal space (e.g., `' '`).
    HorizontalSpace(String),
    /// Vertical space (e.g., `'\n'`).
    VerticalSpace(String),
    /// Text that describes the current parameter when code completion was run on a function call,
    /// message send, or template specialization.
    CurrentParameter(String),
    /// Informative text that should be displayed but not inserted as part of the template.
    Informative(String),
    /// Text that should be replaced by the user.
    Placeholder(String),
    /// Text that specifies the result type of the containing result.
    ResultType(String),
    /// Text that should be inserted.
    Text(String),
    /// Text that the user would be expected to type to get the containing code completion result.
    TypedText(String),
    /// An optional piece that could be part of the template but is not required.
    Optional(CompletionString<'r>),
}

impl<'r> CompletionChunk<'r> {
    //- Accessors --------------------------------

    /// Returns the text associated with this completion chunk if this chunk is not optional.
    pub fn get_text(&self) -> Option<String> {
        match *self {
            CompletionChunk::Colon => Some(":".into()),
            CompletionChunk::Comma => Some(",".into()),
            CompletionChunk::Equals => Some("=".into()),
            CompletionChunk::Semicolon => Some(";".into()),
            CompletionChunk::LeftAngleBracket => Some("<".into()),
            CompletionChunk::RightAngleBracket => Some(">".into()),
            CompletionChunk::LeftBrace => Some("{".into()),
            CompletionChunk::RightBrace => Some("}".into()),
            CompletionChunk::LeftParenthesis => Some("(".into()),
            CompletionChunk::RightParenthesis => Some(")".into()),
            CompletionChunk::LeftSquareBracket => Some("[".into()),
            CompletionChunk::RightSquareBracket => Some("]".into()),
            CompletionChunk::CurrentParameter(ref text) |
            CompletionChunk::Informative(ref text) |
            CompletionChunk::Placeholder(ref text) |
            CompletionChunk::ResultType(ref text) |
            CompletionChunk::TypedText(ref text) |
            CompletionChunk::Text(ref text) |
            CompletionChunk::HorizontalSpace(ref text) |
            CompletionChunk::VerticalSpace(ref text) => Some(text.clone()),
            CompletionChunk::Optional(_) => None,
        }
    }

    /// Returns whether this completion chunk is optional.
    pub fn is_optional(&self) -> bool {
        match *self {
            CompletionChunk::Optional(_) => true,
            _ => false,
        }
    }
}

//================================================
// Structs
//================================================

// Completer ____________________________________

builder! {
    /// Runs code completion.
    builder Completer: CXCodeComplete_Flags {
        tu: &'tu TranslationUnit<'tu>,
        file: PathBuf,
        line: u32,
        column: u32;
    FIELDS:
        /// Sets the unsaved files to use.
        pub unsaved: (Vec<Unsaved>, &[Unsaved]) = |us: &[Unsaved]| us.to_vec();
    OPTIONS:
        /// Sets whether macros will be included in code completion results.
        pub macros: CXCodeComplete_IncludeMacros,
        /// Sets whether code patterns (e.g., for loops) will be included in code completion
        /// results.
        pub code_patterns: CXCodeComplete_IncludeCodePatterns,
        /// Sets whether documentation comment briefs will be included in code completion results.
        pub briefs: CXCodeComplete_IncludeBriefComments,
    }
}

impl<'tu> Completer<'tu> {
    //- Constructors -----------------------------

    #[doc(hidden)]
    pub fn new<F: Into<PathBuf>>(
        tu: &'tu TranslationUnit<'tu>, file: F, line: u32, column: u32
    ) -> Completer<'tu> {
        let file = file.into();
        let flags = unsafe { ffi::clang_defaultCodeCompleteOptions() };
        Completer {  tu: tu, file: file, line: line, column: column, unsaved: vec![], flags: flags }
    }

    //- Accessors --------------------------------

    /// Runs code completion.
    pub fn complete(&self) -> CompletionResults {
        unsafe {
            let ptr = ffi::clang_codeCompleteAt(
                self.tu.ptr,
                utility::from_path(&self.file).as_ptr(),
                self.line as c_uint,
                self.column as c_uint,
                mem::transmute(self.unsaved.as_ptr()),
                self.unsaved.len() as c_uint,
                self.flags,
            );
            CompletionResults::from_ptr(ptr)
        }
    }
}

// CompletionContext _____________________________

options! {
    /// Indicates which types of results were included in a set of code completion results.
    options CompletionContext: CXCompletionContext {
        /// Indicates whether all possible types were included.
        pub all_types: CXCompletionContext_AnyType,
        /// Indicates whether all possible values were included.
        pub all_values: CXCompletionContext_AnyValue,
        /// Indicates whether values that resolve to C++ class types were included.
        pub class_type_values: CXCompletionContext_CXXClassTypeValue,
        /// Indicates whether the members of a record that are accessed with the dot operator were
        /// included.
        pub dot_members: CXCompletionContext_DotMemberAccess,
        /// Indicates whether the members of a record that are accessed with the arrow operator were
        /// included.
        pub arrow_members: CXCompletionContext_ArrowMemberAccess,
        /// Indicates whether enum tags were included.
        pub enum_tags: CXCompletionContext_EnumTag,
        /// Indicates whether union tags were included.
        pub union_tags: CXCompletionContext_UnionTag,
        /// Indicates whether struct tags were included.
        pub struct_tags: CXCompletionContext_StructTag,
        /// Indicates whether C++ class names were included.
        pub class_names: CXCompletionContext_ClassTag,
        /// Indicates whether C++ namespaces and namespace aliases were included.
        pub namespaces: CXCompletionContext_Namespace,
        /// Indicates whether C++ nested name specifiers were included.
        pub nested_name_specifiers: CXCompletionContext_NestedNameSpecifier,
        /// Indicates whether macro names were included.
        pub macro_names: CXCompletionContext_MacroName,
        /// Indicates whether natural language results were included.
        pub natural_language: CXCompletionContext_NaturalLanguage,
        /// Indicates whether values that resolve to Objective-C objects were included.
        pub objc_object_values: CXCompletionContext_ObjCObjectValue,
        /// Indicates whether values that resolve to Objective-C selectors were included.
        pub objc_selector_values: CXCompletionContext_ObjCSelectorValue,
        /// Indicates whether the properties of an Objective-C object that are accessed with the dot
        /// operator were included.
        pub objc_property_members: CXCompletionContext_ObjCPropertyAccess,
        /// Indicates whether Objective-C interfaces were included.
        pub objc_interfaces: CXCompletionContext_ObjCInterface,
        /// Indicates whether Objective-C protocols were included.
        pub objc_protocols: CXCompletionContext_ObjCProtocol,
        /// Indicates whether Objective-C categories were included.
        pub objc_categories: CXCompletionContext_ObjCCategory,
        /// Indicates whether Objective-C instance messages were included.
        pub objc_instance_messages: CXCompletionContext_ObjCInstanceMessage,
        /// Indicates whether Objective-C class messages were included.
        pub objc_class_messages: CXCompletionContext_ObjCClassMessage,
        /// Indicates whether Objective-C selector names were included.
        pub objc_selector_names: CXCompletionContext_ObjCSelectorName,
    }
}

// CompletionResult ______________________________

/// A code completion result.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CompletionResult<'r> {
    /// The categorization of the AST entity this code completion result produces.
    pub kind: EntityKind,
    /// The completion string for this code completion result.
    pub string: CompletionString<'r>,
}

impl<'r> CompletionResult<'r> {
    //- Constructors -----------------------------

    fn from_raw(raw: ffi::CXCompletionResult) -> CompletionResult<'r> {
        let kind = unsafe { mem::transmute(raw.CursorKind) };
        CompletionResult { kind: kind, string: CompletionString::from_raw(raw.CompletionString) }
    }
}

impl<'r> cmp::PartialOrd for CompletionResult<'r> {
    fn partial_cmp(&self, other: &CompletionResult<'r>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'r> cmp::Ord for CompletionResult<'r> {
    fn cmp(&self, other: &CompletionResult<'r>) -> Ordering {
        self.string.cmp(&other.string)
    }
}

// CompletionResults _____________________________

/// A set of code completion results.
pub struct CompletionResults {
    ptr: *mut ffi::CXCodeCompleteResults,
}

impl CompletionResults {
    //- Constructors -----------------------------

    fn from_ptr(ptr: *mut ffi::CXCodeCompleteResults) -> CompletionResults {
        CompletionResults { ptr: ptr }
    }

    //- Accessors --------------------------------

    /// Returns the diagnostics that were produced prior to the code completion context for this set
    /// of code completion results.
    pub fn get_diagnostics<'tu>(&self, tu: &'tu TranslationUnit<'tu>) -> Vec<Diagnostic<'tu>> {
        iter!(
            clang_codeCompleteGetNumDiagnostics(self.ptr),
            clang_codeCompleteGetDiagnostic(self.ptr),
        ).map(|d| Diagnostic::from_ptr(d, tu)).collect()
    }

    /// Returns the code completion context for this set of code completion results, if any.
    pub fn get_context(&self) -> Option<CompletionContext> {
        let bits = unsafe { ffi::clang_codeCompleteGetContexts(self.ptr) as c_uint };
        if bits != 0 && bits != ffi::CXCompletionContext_Unknown.bits() {
            Some(CompletionContext::from(ffi::CXCompletionContext::from_bits_truncate(bits)))
        } else {
            None
        }
    }

    /// Returns the categorization of the entity that contains the code completion context for this
    /// set of code completion results and whether that entity is incomplete, if applicable.
    pub fn get_container_kind(&self) -> Option<(EntityKind, bool)> {
        unsafe {
            let mut incomplete = mem::uninitialized();
            match ffi::clang_codeCompleteGetContainerKind(self.ptr, &mut incomplete) {
                ffi::CXCursorKind::InvalidCode => None,
                other => Some((mem::transmute(other), incomplete != 0)),
            }
        }
    }

    /// Returns the selector or partial selector that has been entered this far for the Objective-C
    /// message send context for this set of code completion results.
    pub fn get_objc_selector(&self) -> Option<String> {
        unsafe { utility::to_string_option(ffi::clang_codeCompleteGetObjCSelector(self.ptr)) }
    }

    /// Returns the USR for the entity that contains the code completion context for this set of
    /// code completion results, if applicable.
    pub fn get_usr(&self) -> Option<Usr> {
        let usr = unsafe { ffi::clang_codeCompleteGetContainerUSR(self.ptr) };
        utility::to_string_option(usr).map(Usr)
    }

    /// Returns the code completion results in this set of code completion results.
    pub fn get_results(&self) -> Vec<CompletionResult> {
        unsafe {
            let raws = slice::from_raw_parts((*self.ptr).Results, (*self.ptr).NumResults as usize);
            raws.iter().cloned().map(CompletionResult::from_raw).collect()
        }
    }
}

impl Drop for CompletionResults {
    fn drop(&mut self) {
        unsafe { ffi::clang_disposeCodeCompleteResults(self.ptr); }
    }
}

impl fmt::Debug for CompletionResults {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.debug_struct("CompletionResults")
            .field("results", &self.get_results())
            .finish()
    }
}

// CompletionString ______________________________

/// A semantic string that describes a code completion result.
#[derive(Copy, Clone)]
pub struct CompletionString<'r> {
    raw: ffi::CXCompletionString,
    _marker: PhantomData<&'r CompletionResults>
}

impl<'r> CompletionString<'r> {
    //- Constructors -----------------------------

    #[doc(hidden)]
    pub fn from_raw(raw: ffi::CXCompletionString) -> CompletionString<'r> {
        CompletionString { raw: raw, _marker: PhantomData }
    }

    //- Accessors --------------------------------

    /// Returns an integer that represents how likely a user is to select this completion string as
    /// determined by internal heuristics. Smaller values indicate higher priorities.
    pub fn get_priority(&self) -> usize {
        unsafe { ffi::clang_getCompletionPriority(self.raw) as usize }
    }

    /// Returns the annotations associated with this completion string.
    pub fn get_annotations(&self) -> Vec<String> {
        iter!(
            clang_getCompletionNumAnnotations(self.raw),
            clang_getCompletionAnnotation(self.raw),
        ).map(utility::to_string).collect()
    }

    /// Returns the availability of this completion string.
    pub fn get_availability(&self) -> Availability {
        unsafe { mem::transmute(ffi::clang_getCompletionAvailability(self.raw)) }
    }

    /// Returns the documentation comment brief associated with the declaration this completion
    /// string refers to, if applicable.
    pub fn get_comment_brief(&self) -> Option<String> {
        unsafe { utility::to_string_option(ffi::clang_getCompletionBriefComment(self.raw)) }
    }

    /// Returns the name of the semantic parent of the declaration this completion string refers to,
    /// if applicable.
    pub fn get_parent_name(&self) -> Option<String> {
        let name = unsafe { ffi::clang_getCompletionParent(self.raw, ptr::null_mut()) };
        utility::to_string_option(name)
    }

    /// Returns the text of the typed text chunk for this completion string, if any.
    pub fn get_typed_text(&self) -> Option<String> {
        for chunk in self.get_chunks() {
            if let CompletionChunk::TypedText(text) = chunk {
                return Some(text);
            }
        }
        None
    }

    /// Returns the chunks of this completion string.
    pub fn get_chunks(&self) -> Vec<CompletionChunk> {
        iter!(
            clang_getNumCompletionChunks(self.raw),
            clang_getCompletionChunkKind(self.raw),
        ).enumerate().map(|(i, k)| {
            macro_rules! text {
                ($variant:ident) => ({
                    let text = unsafe { ffi::clang_getCompletionChunkText(self.raw, i as c_uint) };
                    CompletionChunk::$variant(utility::to_string(text))
                });
            }

            match k {
                ffi::CXCompletionChunkKind::Colon => CompletionChunk::Colon,
                ffi::CXCompletionChunkKind::Comma => CompletionChunk::Comma,
                ffi::CXCompletionChunkKind::Equal => CompletionChunk::Equals,
                ffi::CXCompletionChunkKind::SemiColon => CompletionChunk::Semicolon,
                ffi::CXCompletionChunkKind::LeftAngle => CompletionChunk::LeftAngleBracket,
                ffi::CXCompletionChunkKind::RightAngle => CompletionChunk::RightAngleBracket,
                ffi::CXCompletionChunkKind::LeftBrace => CompletionChunk::LeftBrace,
                ffi::CXCompletionChunkKind::RightBrace => CompletionChunk::RightBrace,
                ffi::CXCompletionChunkKind::LeftParen => CompletionChunk::LeftParenthesis,
                ffi::CXCompletionChunkKind::RightParen => CompletionChunk::RightParenthesis,
                ffi::CXCompletionChunkKind::LeftBracket => CompletionChunk::LeftSquareBracket,
                ffi::CXCompletionChunkKind::RightBracket => CompletionChunk::RightSquareBracket,
                ffi::CXCompletionChunkKind::HorizontalSpace => text!(HorizontalSpace),
                ffi::CXCompletionChunkKind::VerticalSpace => text!(VerticalSpace),
                ffi::CXCompletionChunkKind::CurrentParameter => text!(CurrentParameter),
                ffi::CXCompletionChunkKind::TypedText => text!(TypedText),
                ffi::CXCompletionChunkKind::Text => text!(Text),
                ffi::CXCompletionChunkKind::Placeholder => text!(Placeholder),
                ffi::CXCompletionChunkKind::Informative => text!(Informative),
                ffi::CXCompletionChunkKind::ResultType => text!(ResultType),
                ffi::CXCompletionChunkKind::Optional => {
                    let i = i as c_uint;
                    let raw = unsafe { ffi::clang_getCompletionChunkCompletionString(self.raw, i) };
                    CompletionChunk::Optional(CompletionString::from_raw(raw))
                },
            }
        }).collect()
    }
}

impl<'r> fmt::Debug for CompletionString<'r> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.debug_struct("CompletionString")
            .field("chunks", &self.get_chunks())
            .finish()
    }
}

impl<'r> cmp::PartialEq for CompletionString<'r> {
    fn eq(&self, other: &CompletionString<'r>) -> bool {
        self.get_chunks() == other.get_chunks()
    }
}

impl<'r> cmp::Eq for CompletionString<'r> { }

impl<'r> cmp::PartialOrd for CompletionString<'r> {
    fn partial_cmp(&self, other: &CompletionString<'r>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'r> cmp::Ord for CompletionString<'r> {
    fn cmp(&self, other: &CompletionString<'r>) -> Ordering {
        match self.get_priority().cmp(&other.get_priority()) {
            Ordering::Equal => self.get_typed_text().cmp(&other.get_typed_text()),
            other => other,
        }
    }
}
