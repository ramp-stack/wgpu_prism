use simplecss::{DeclarationTokenizer, StyleSheet};
use usvg::{BlendMode, Group, Node, Paint, Tree};

use std::io::Read;
use std::sync::Arc;

use super::Size;

#[derive(Clone, Debug)]
pub struct Svg(Arc<Tree>);
impl Svg {
    pub fn read<R: Read>(mut reader: R) -> Result<Self, String> {
        let mut source = String::new();
        reader.read_to_string(&mut source).map_err(|error| error.to_string())?;
        let document = usvg::roxmltree::Document::parse(&source)
            .map_err(|error| error.to_string())?;
        // Disabled resolvers can remove unsupported nodes before the parsed tree is validated.
        for node in document.descendants() {
            let name = node.tag_name().name();
            if matches!(name,
                "linearGradient" | "radialGradient" | "pattern" | "mask" | "filter"
                    | "image" | "text" | "foreignObject") {
                return Err(format!("SVG element <{name}> is not supported"))
            }
            if let Some(value) = node.attribute("color-interpolation") {
                Self::validate_interpolation(value)?;
            }
            for property in DeclarationTokenizer::from(node.attribute("style").unwrap_or_default()) {
                if property.name.eq_ignore_ascii_case("color-interpolation") {
                    Self::validate_interpolation(property.value)?;
                }
            }
            if name == "style" {
                let sheet = StyleSheet::parse(node.text().unwrap_or_default());
                for property in sheet.rules.iter().flat_map(|rule| &rule.declarations) {
                    if property.name.eq_ignore_ascii_case("color-interpolation") {
                        Self::validate_interpolation(property.value)?;
                    }
                }
            }
        }
        let options = usvg::Options {
            image_href_resolver: usvg::ImageHrefResolver {
                resolve_data: Box::new(|_, _, _| None),
                resolve_string: Box::new(|_, _| None),
            },
            ..Default::default()
        };
        let tree = Tree::from_xmltree(&document, &options).map_err(|error| error.to_string())?;
        Self::validate(tree.root())?;
        for clip in tree.clip_paths() {Self::validate(clip.root())?;}
        Ok(Self(Arc::new(tree)))
    }
    pub fn dimensions(&self) -> Size {(self.0.size().width(), self.0.size().height())}

    fn validate_interpolation(value: &str) -> Result<(), String> {
        if ["sRGB", "auto", "inherit", "initial", "unset", "revert", "revert-layer"]
            .iter().any(|supported| value.trim().eq_ignore_ascii_case(supported)) {return Ok(())}
        Err(format!("SVG color-interpolation '{value}' is not supported; use sRGB"))
    }

    fn validate(group: &Group) -> Result<(), String> {
        if group.blend_mode() != BlendMode::Normal {
            return Err("SVG blend modes other than normal are not supported".into())
        }
        if group.mask().is_some() || !group.filters().is_empty() {
            return Err("SVG masks and filters are not supported".into())
        }
        for child in group.children() {
            match child {
                Node::Group(group) => Self::validate(group)?,
                Node::Path(path) => {
                    if path.fill().is_some_and(|fill| !matches!(fill.paint(), Paint::Color(_)))
                        || path.stroke().is_some_and(|stroke| !matches!(stroke.paint(), Paint::Color(_))) {
                        return Err("SVG gradients and patterns are not supported".into())
                    }
                    if path.stroke().is_some_and(|stroke| stroke.dasharray().is_some()) {
                        return Err("SVG dashed strokes are not supported".into())
                    }
                }
                Node::Image(_) | Node::Text(_) => {
                    return Err("SVG images and text are not supported".into())
                }
            }
        }
        Ok(())
    }
}
impl AsRef<Tree> for Svg {
    fn as_ref(&self) -> &Tree {self.0.as_ref()}
}
impl PartialEq for Svg {
    fn eq(&self, other: &Self) -> bool {Arc::ptr_eq(&self.0, &other.0)}
}
impl Eq for Svg {}
