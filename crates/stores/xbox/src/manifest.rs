//! A tiny, forgiving extractor for the few facts we need from `AppxManifest.xml` and
//! `MicrosoftGame.config`. It is not an XML parser: it scans start tags and their attributes, so
//! malformed input yields fewer facts instead of errors or panics.

/// One start tag with its attributes. Names are lower-cased local names (namespace prefix removed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    /// Byte offset just after the closing `>`.
    pub end: usize,
    pub self_closing: bool,
}

impl Tag {
    /// Attribute value by (case-insensitive, prefix-less) name; empty values count as absent.
    pub fn attr(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.attrs
            .iter()
            .find(|(n, v)| *n == name && !v.trim().is_empty())
            .map(|(_, v)| v.trim())
    }
}

fn local_name(raw: &str) -> String {
    raw.rsplit(':').next().unwrap_or(raw).to_ascii_lowercase()
}

/// Every start tag of the document, in order. Comments, processing instructions, CDATA and end
/// tags are skipped.
pub fn tags(xml: &str) -> Vec<Tag> {
    let bytes = xml.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = xml[i..].find('<') {
        let start = i + off;
        let rest = &xml[start..];
        if rest.starts_with("<!--") {
            i = skip_past(xml, start, "-->");
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            i = skip_past(xml, start, "]]>");
            continue;
        }
        if rest.starts_with("<?") {
            i = skip_past(xml, start, "?>");
            continue;
        }
        if rest.starts_with("</") || rest.starts_with("<!") {
            i = skip_past(xml, start, ">");
            continue;
        }
        // Start tag: name.
        let mut p = start + 1;
        while p < bytes.len() && !matches!(bytes[p], b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>') {
            p += 1;
        }
        let name = local_name(&xml[start + 1..p]);
        let mut attrs = Vec::new();
        let mut self_closing = false;
        let mut closed = false;
        while p < bytes.len() {
            while p < bytes.len() && bytes[p].is_ascii_whitespace() {
                p += 1;
            }
            match bytes.get(p) {
                None => break,
                Some(b'>') => {
                    p += 1;
                    closed = true;
                    break;
                }
                Some(b'/') => {
                    self_closing = true;
                    p += 1;
                    continue;
                }
                _ => {}
            }
            let name_start = p;
            while p < bytes.len()
                && !matches!(bytes[p], b'=' | b'/' | b'>')
                && !bytes[p].is_ascii_whitespace()
            {
                p += 1;
            }
            let attr_name = local_name(&xml[name_start..p]);
            while p < bytes.len() && bytes[p].is_ascii_whitespace() {
                p += 1;
            }
            if bytes.get(p) != Some(&b'=') {
                // Valueless attribute (not valid XML): ignore it.
                if p == name_start {
                    p += 1;
                }
                continue;
            }
            p += 1;
            while p < bytes.len() && bytes[p].is_ascii_whitespace() {
                p += 1;
            }
            let quote = match bytes.get(p) {
                Some(q @ (b'"' | b'\'')) => *q,
                _ => break,
            };
            p += 1;
            let value_start = p;
            while p < bytes.len() && bytes[p] != quote {
                p += 1;
            }
            if p >= bytes.len() {
                break;
            }
            attrs.push((attr_name, decode_entities(&xml[value_start..p])));
            p += 1;
        }
        if closed && !name.is_empty() {
            out.push(Tag {
                name,
                attrs,
                end: p,
                self_closing,
            });
        }
        i = p.max(start + 1);
    }
    out
}

fn skip_past(xml: &str, from: usize, needle: &str) -> usize {
    match xml[from..].find(needle) {
        Some(off) => from + off + needle.len(),
        None => xml.len(),
    }
}

/// First tag with the given local name.
pub fn first<'a>(tags: &'a [Tag], name: &str) -> Option<&'a Tag> {
    tags.iter().find(|t| t.name == name)
}

/// Trimmed, decoded text of the first non-empty `<name>text</name>` element.
pub fn element_text(xml: &str, tags: &[Tag], name: &str) -> Option<String> {
    tags.iter()
        .filter(|t| t.name == name && !t.self_closing)
        .find_map(|t| {
            let rest = &xml[t.end..];
            let text = rest[..rest.find('<').unwrap_or(rest.len())].trim();
            (!text.is_empty()).then(|| decode_entities(text))
        })
}

/// Decodes the five predefined XML entities and numeric character references.
pub fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        rest = &rest[pos..];
        let decoded = rest.find(';').filter(|&semi| semi <= 10).and_then(|semi| {
            let entity = &rest[1..semi];
            let ch = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                    .map(|hex| u32::from_str_radix(hex, 16))
                    .or_else(|| entity.strip_prefix('#').map(|dec| dec.parse::<u32>()))
                    .and_then(Result::ok)
                    .and_then(char::from_u32),
            };
            ch.map(|c| (c, semi + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `ms-resource:` references point into the package's resource index (a binary file), which we
/// do not read.
pub fn is_resource_ref(value: &str) -> bool {
    value
        .trim()
        .get(..12)
        .is_some_and(|p| p.eq_ignore_ascii_case("ms-resource:"))
}

fn literal(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty() && !is_resource_ref(v))
}

/// Facts from `AppxManifest.xml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppxManifest {
    pub identity_name: Option<String>,
    pub publisher: Option<String>,
    /// `Application Id` values in document order.
    pub app_ids: Vec<String>,
    /// `Properties/DisplayName` when it is literal text (not an `ms-resource:` reference).
    pub display_name: Option<String>,
    /// `VisualElements DisplayName` when literal.
    pub visual_display_name: Option<String>,
    /// Logo paths relative to the package root, biggest tile first.
    pub logos: Vec<String>,
    /// A framework or resource package (never a game).
    pub is_framework_or_resource: bool,
}

pub fn parse_appx_manifest(xml: &str) -> AppxManifest {
    let all = tags(xml);
    let identity = first(&all, "identity");
    let visual = first(&all, "visualelements");
    let mut logos = Vec::new();
    if let Some(v) = visual {
        for attr in ["square310x310logo", "square150x150logo"] {
            logos.extend(v.attr(attr).map(str::to_string));
        }
    }
    logos.extend(element_text(xml, &all, "logo"));
    if let Some(v) = visual {
        logos.extend(v.attr("square44x44logo").map(str::to_string));
    }
    let flag = |name: &str| {
        element_text(xml, &all, name).is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
    };
    AppxManifest {
        identity_name: identity.and_then(|t| t.attr("name")).map(str::to_string),
        publisher: identity
            .and_then(|t| t.attr("publisher"))
            .map(str::to_string),
        app_ids: all
            .iter()
            .filter(|t| t.name == "application")
            .filter_map(|t| t.attr("id").map(str::to_string))
            .collect(),
        display_name: literal(element_text(xml, &all, "displayname")),
        visual_display_name: literal(
            visual
                .and_then(|t| t.attr("displayname"))
                .map(str::to_string),
        ),
        logos,
        is_framework_or_resource: flag("framework") || flag("resourcepackage"),
    }
}

/// Facts from `MicrosoftGame.config` (GDK titles).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GameConfig {
    pub identity_name: Option<String>,
    /// `ShellVisuals DefaultDisplayName` when literal.
    pub display_name: Option<String>,
    /// Logo paths relative to the config's folder, biggest tile first.
    pub logos: Vec<String>,
}

pub fn parse_game_config(xml: &str) -> GameConfig {
    let all = tags(xml);
    let visuals = first(&all, "shellvisuals");
    let mut logos = Vec::new();
    if let Some(v) = visuals {
        for attr in ["square480x480logo", "square150x150logo", "storelogo"] {
            logos.extend(v.attr(attr).map(str::to_string));
        }
    }
    GameConfig {
        identity_name: first(&all, "identity")
            .and_then(|t| t.attr("name"))
            .map(str::to_string),
        display_name: literal(
            visuals
                .and_then(|t| t.attr("defaultdisplayname"))
                .map(str::to_string),
        ),
        logos,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!-- <Identity Name="Commented.Out" /> -->
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
         xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10">
  <Identity Name="Contoso.Racer" Publisher="CN=Contoso &amp; Co, O=Contoso" Version="1.2.3.0" />
  <Properties>
    <DisplayName>ms-resource:AppName</DisplayName>
    <Logo>Assets\StoreLogo.png</Logo>
  </Properties>
  <Applications>
    <Application Id="Game" Executable="Racer.exe">
      <uap:VisualElements DisplayName='Racer Deluxe' Square150x150Logo="Assets\Logo150.png" Square44x44Logo="Assets\Logo44.png"/>
    </Application>
    <Application Id="Editor" Executable="Editor.exe" />
  </Applications>
</Package>"#;

    #[test]
    fn extracts_manifest_facts() {
        let m = parse_appx_manifest(MANIFEST);
        assert_eq!(m.identity_name.as_deref(), Some("Contoso.Racer"));
        assert_eq!(m.publisher.as_deref(), Some("CN=Contoso & Co, O=Contoso"));
        assert_eq!(m.app_ids, vec!["Game", "Editor"]);
        assert_eq!(m.display_name, None, "ms-resource references are not names");
        assert_eq!(m.visual_display_name.as_deref(), Some("Racer Deluxe"));
        assert_eq!(
            m.logos,
            vec![
                r"Assets\Logo150.png",
                r"Assets\StoreLogo.png",
                r"Assets\Logo44.png"
            ]
        );
        assert!(!m.is_framework_or_resource);
    }

    #[test]
    fn literal_display_name_and_framework_flag() {
        let xml = r#"<Package><Identity Name="A.B"/><Properties>
            <DisplayName>Plain &#38; Simple</DisplayName><Framework>true</Framework></Properties></Package>"#;
        let m = parse_appx_manifest(xml);
        assert_eq!(m.display_name.as_deref(), Some("Plain & Simple"));
        assert!(m.is_framework_or_resource);
        let res = parse_appx_manifest(
            "<Package><Properties><ResourcePackage>true</ResourcePackage></Properties></Package>",
        );
        assert!(res.is_framework_or_resource);
    }

    #[test]
    fn extracts_game_config() {
        let xml = r#"<Game configVersion="1">
  <Identity Name="Contoso.Racer" Publisher="CN=X" Version="1.0.0.0"/>
  <ShellVisuals DefaultDisplayName="Racer: The Game" PublisherDisplayName="Contoso"
     StoreLogo="Assets\Store.png" Square150x150Logo="Assets\150.png" Square480x480Logo="Assets\480.png"/>
</Game>"#;
        let c = parse_game_config(xml);
        assert_eq!(c.identity_name.as_deref(), Some("Contoso.Racer"));
        assert_eq!(c.display_name.as_deref(), Some("Racer: The Game"));
        assert_eq!(
            c.logos,
            vec![r"Assets\480.png", r"Assets\150.png", r"Assets\Store.png"]
        );
    }

    #[test]
    fn malformed_input_does_not_panic() {
        for xml in [
            "",
            "<",
            "<<<>>>",
            "<Identity Name=",
            "<Identity Name=\"unterminated",
            "<Identity Name=\"x\"",
            "<!-- never closed <Identity Name=\"x\"/>",
            "<![CDATA[ <Identity/>",
            "<a b c = d e='f' / >",
            "&#xZZ; &amp &#99999999999; <DisplayName>&bogus;</DisplayName>",
            "<DisplayName>ünï</DisplayName><Identity Name=\"ünï\"/>",
            "<Application Id=\"\"/>",
        ] {
            let _ = parse_appx_manifest(xml);
            let _ = parse_game_config(xml);
        }
        assert_eq!(
            parse_appx_manifest("<Identity Name=\"x\""),
            AppxManifest::default()
        );
    }

    #[test]
    fn decodes_entities() {
        assert_eq!(
            decode_entities("a &lt;b&gt; &quot;c&quot; &apos;d&apos; &#65;&#x42;"),
            "a <b> \"c\" 'd' AB"
        );
        assert_eq!(
            decode_entities("fish & chips &nope;"),
            "fish & chips &nope;"
        );
    }

    #[test]
    fn recognises_resource_refs() {
        assert!(is_resource_ref("ms-resource:AppName"));
        assert!(is_resource_ref("  MS-RESOURCE:/Files/x"));
        assert!(!is_resource_ref("Halo"));
        assert!(!is_resource_ref("ü"));
    }
}
