pub mod cli;
pub mod english;
pub mod frequency;
pub mod paths;
pub mod swedish;

use quick_xml::XmlVersion;
use quick_xml::events::BytesStart;

/// Return the unescaped value of attribute `name`, if present.
pub(crate) fn attr(element: &BytesStart, name: &str) -> Option<String> {
    element
        .try_get_attribute(name)
        .ok()
        .flatten()
        .and_then(|attribute| attribute.normalized_value(XmlVersion::Implicit1_0).ok())
        .map(|value| value.into_owned())
}
