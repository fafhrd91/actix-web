//! # References
//!
//! "The Content-Disposition Header Field" https://www.ietf.org/rfc/rfc2183.txt
//! "The Content-Disposition Header Field in the Hypertext Transfer Protocol (HTTP)" https://www.ietf.org/rfc/rfc6266.txt
//! "Returning Values from Forms: multipart/form-data" https://www.ietf.org/rfc/rfc7578.txt
//! Browser conformance tests at: http://greenbytes.de/tech/tc2231/
//! IANA assignment: http://www.iana.org/assignments/cont-disp/cont-disp.xhtml

use once_cell::sync::Lazy;
use regex::Regex;
use std::fmt::{self, Write};

use crate::header::{self, ExtendedValue, Header, IntoHeaderValue, Writer};

/// Split at the index of the first `needle` if it exists or at the end.
fn split_once(haystack: &str, needle: char) -> (&str, &str) {
    haystack.find(needle).map_or_else(
        || (haystack, ""),
        |sc| {
            let (first, last) = haystack.split_at(sc);
            (first, last.split_at(1).1)
        },
    )
}

/// Split at the index of the first `needle` if it exists or at the end, trim the right of the
/// first part and the left of the last part.
fn split_once_and_trim(haystack: &str, needle: char) -> (&str, &str) {
    let (first, last) = split_once(haystack, needle);
    (first.trim_end(), last.trim_start())
}

/// The implied disposition of the content of the HTTP body.
#[derive(Clone, Debug, PartialEq)]
pub enum DispositionType {
    /// Inline implies default processing
    Inline,
    /// Attachment implies that the recipient should prompt the user to save the response locally,
    /// rather than process it normally (as per its media type).
    Attachment,
    /// Used in *multipart/form-data* as defined in
    /// [RFC7578](https://tools.ietf.org/html/rfc7578) to carry the field name and the file name.
    FormData,
    /// Extension type. Should be handled by recipients the same way as Attachment
    Ext(String),
}

impl<'a> From<&'a str> for DispositionType {
    fn from(origin: &'a str) -> DispositionType {
        if origin.eq_ignore_ascii_case("inline") {
            DispositionType::Inline
        } else if origin.eq_ignore_ascii_case("attachment") {
            DispositionType::Attachment
        } else if origin.eq_ignore_ascii_case("form-data") {
            DispositionType::FormData
        } else {
            DispositionType::Ext(origin.to_owned())
        }
    }
}

/// Parameter in [`ContentDisposition`].
///
/// # Examples
/// ```
/// use actix_http::http::header::DispositionParam;
///
/// let param = DispositionParam::Filename(String::from("sample.txt"));
/// assert!(param.is_filename());
/// assert_eq!(param.as_filename().unwrap(), "sample.txt");
/// ```
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum DispositionParam {
    /// For [`DispositionType::FormData`] (i.e. *multipart/form-data*), the name of a field from
    /// the form.
    Name(String),
    /// A plain file name.
    ///
    /// It is [not supposed](https://tools.ietf.org/html/rfc6266#appendix-D) to contain any
    /// non-ASCII characters when used in a *Content-Disposition* HTTP response header, where
    /// [`FilenameExt`](DispositionParam::FilenameExt) with charset UTF-8 may be used instead
    /// in case there are Unicode characters in file names.
    Filename(String),
    /// An extended file name. It must not exist for `ContentType::Formdata` according to
    /// [RFC7578 Section 4.2](https://tools.ietf.org/html/rfc7578#section-4.2).
    FilenameExt(ExtendedValue),
    /// An unrecognized regular parameter as defined in
    /// [RFC5987](https://tools.ietf.org/html/rfc5987) as *reg-parameter*, in
    /// [RFC6266](https://tools.ietf.org/html/rfc6266) as *token "=" value*. Recipients should
    /// ignore unrecognizable parameters.
    Unknown(String, String),
    /// An unrecognized extended parameter as defined in
    /// [RFC5987](https://tools.ietf.org/html/rfc5987) as *ext-parameter*, in
    /// [RFC6266](https://tools.ietf.org/html/rfc6266) as *ext-token "=" ext-value*. The single
    /// trailing asterisk is not included. Recipients should ignore unrecognizable parameters.
    UnknownExt(String, ExtendedValue),
}

impl DispositionParam {
    /// Returns `true` if the parameter is [`Name`](DispositionParam::Name).
    #[inline]
    pub fn is_name(&self) -> bool {
        self.as_name().is_some()
    }

    /// Returns `true` if the parameter is [`Filename`](DispositionParam::Filename).
    #[inline]
    pub fn is_filename(&self) -> bool {
        self.as_filename().is_some()
    }

    /// Returns `true` if the parameter is [`FilenameExt`](DispositionParam::FilenameExt).
    #[inline]
    pub fn is_filename_ext(&self) -> bool {
        self.as_filename_ext().is_some()
    }

    /// Returns `true` if the parameter is [`Unknown`](DispositionParam::Unknown) and the `name`
    #[inline]
    /// matches.
    pub fn is_unknown<T: AsRef<str>>(&self, name: T) -> bool {
        self.as_unknown(name).is_some()
    }

    /// Returns `true` if the parameter is [`UnknownExt`](DispositionParam::UnknownExt) and the
    /// `name` matches.
    #[inline]
    pub fn is_unknown_ext<T: AsRef<str>>(&self, name: T) -> bool {
        self.as_unknown_ext(name).is_some()
    }

    /// Returns the name if applicable.
    #[inline]
    pub fn as_name(&self) -> Option<&str> {
        match self {
            DispositionParam::Name(ref name) => Some(name.as_str()),
            _ => None,
        }
    }

    /// Returns the filename if applicable.
    #[inline]
    pub fn as_filename(&self) -> Option<&str> {
        match self {
            DispositionParam::Filename(ref filename) => Some(filename.as_str()),
            _ => None,
        }
    }

    /// Returns the filename* if applicable.
    #[inline]
    pub fn as_filename_ext(&self) -> Option<&ExtendedValue> {
        match self {
            DispositionParam::FilenameExt(ref value) => Some(value),
            _ => None,
        }
    }

    /// Returns the value of the unrecognized regular parameter if it is
    /// [`Unknown`](DispositionParam::Unknown) and the `name` matches.
    #[inline]
    pub fn as_unknown<T: AsRef<str>>(&self, name: T) -> Option<&str> {
        match self {
            DispositionParam::Unknown(ref ext_name, ref value)
                if ext_name.eq_ignore_ascii_case(name.as_ref()) =>
            {
                Some(value.as_str())
            }
            _ => None,
        }
    }

    /// Returns the value of the unrecognized extended parameter if it is
    /// [`Unknown`](DispositionParam::Unknown) and the `name` matches.
    #[inline]
    pub fn as_unknown_ext<T: AsRef<str>>(&self, name: T) -> Option<&ExtendedValue> {
        match self {
            DispositionParam::UnknownExt(ref ext_name, ref value)
                if ext_name.eq_ignore_ascii_case(name.as_ref()) =>
            {
                Some(value)
            }
            _ => None,
        }
    }
}

/// A *Content-Disposition* header. It is compatible to be used either as
/// [a response header for the main body](https://mdn.io/Content-Disposition#As_a_response_header_for_the_main_body)
/// as (re)defined in [RFC6266](https://tools.ietf.org/html/rfc6266), or as
/// [a header for a multipart body](https://mdn.io/Content-Disposition#As_a_header_for_a_multipart_body)
/// as (re)defined in [RFC7587](https://tools.ietf.org/html/rfc7578).
///
/// In a regular HTTP response, the *Content-Disposition* response header is a header indicating if
/// the content is expected to be displayed *inline* in the browser, that is, as a Web page or as
/// part of a Web page, or as an attachment, that is downloaded and saved locally, and also can be
/// used to attach additional metadata, such as the filename to use when saving the response payload
/// locally.
///
/// In a *multipart/form-data* body, the HTTP *Content-Disposition* general header is a header that
/// can be used on the subpart of a multipart body to give information about the field it applies to.
/// The subpart is delimited by the boundary defined in the *Content-Type* header. Used on the body
/// itself, *Content-Disposition* has no effect.
///
/// # ABNF

/// ```text
/// content-disposition = "Content-Disposition" ":"
///                       disposition-type *( ";" disposition-parm )
///
/// disposition-type    = "inline" | "attachment" | disp-ext-type
///                       ; case-insensitive
///
/// disp-ext-type       = token
///
/// disposition-parm    = filename-parm | disp-ext-parm
///
/// filename-parm       = "filename" "=" value
///                     | "filename*" "=" ext-value
///
/// disp-ext-parm       = token "=" value
///                     | ext-token "=" ext-value
///
/// ext-token           = <the characters in token, followed by "*">
/// ```
///
/// # Note
///
/// filename is [not supposed](https://tools.ietf.org/html/rfc6266#appendix-D) to contain any
/// non-ASCII characters when used in a *Content-Disposition* HTTP response header, where
/// filename* with charset UTF-8 may be used instead in case there are Unicode characters in file
/// names.
/// filename is [acceptable](https://tools.ietf.org/html/rfc7578#section-4.2) to be UTF-8 encoded
/// directly in a *Content-Disposition* header for *multipart/form-data*, though.
///
/// filename* [must not](https://tools.ietf.org/html/rfc7578#section-4.2) be used within
/// *multipart/form-data*.
///
/// # Example
///
/// ```
/// use actix_http::http::header::{
///     Charset, ContentDisposition, DispositionParam, DispositionType,
///     ExtendedValue,
/// };
///
/// let cd1 = ContentDisposition {
///     disposition: DispositionType::Attachment,
///     parameters: vec![DispositionParam::FilenameExt(ExtendedValue {
///         charset: Charset::Iso_8859_1, // The character set for the bytes of the filename
///         language_tag: None, // The optional language tag (see `language-tag` crate)
///         value: b"\xa9 Copyright 1989.txt".to_vec(), // the actual bytes of the filename
///     })],
/// };
/// assert!(cd1.is_attachment());
/// assert!(cd1.get_filename_ext().is_some());
///
/// let cd2 = ContentDisposition {
///     disposition: DispositionType::FormData,
///     parameters: vec![
///         DispositionParam::Name(String::from("file")),
///         DispositionParam::Filename(String::from("bill.odt")),
///     ],
/// };
/// assert_eq!(cd2.get_name(), Some("file")); // field name
/// assert_eq!(cd2.get_filename(), Some("bill.odt"));
///
/// // HTTP response header with Unicode characters in file names
/// let cd3 = ContentDisposition {
///     disposition: DispositionType::Attachment,
///     parameters: vec![
///         DispositionParam::FilenameExt(ExtendedValue {
///             charset: Charset::Ext(String::from("UTF-8")),
///             language_tag: None,
///             value: String::from("\u{1f600}.svg").into_bytes(),
///         }),
///         // fallback for better compatibility
///         DispositionParam::Filename(String::from("Grinning-Face-Emoji.svg"))
///     ],
/// };
/// assert_eq!(cd3.get_filename_ext().map(|ev| ev.value.as_ref()),
///            Some("\u{1f600}.svg".as_bytes()));
/// ```
///
/// # Security Note
///
/// If "filename" parameter is supplied, do not use the file name blindly, check and possibly
/// change to match local file system conventions if applicable, and do not use directory path
/// information that may be present. See [RFC2183](https://tools.ietf.org/html/rfc2183#section-2.3).
#[derive(Clone, Debug, PartialEq)]
pub struct ContentDisposition {
    /// The disposition type
    pub disposition: DispositionType,
    /// Disposition parameters
    pub parameters: Vec<DispositionParam>,
}

impl ContentDisposition {
    /// Parse a raw Content-Disposition header value.
    pub fn from_raw(hv: &header::HeaderValue) -> Result<Self, crate::error::ParseError> {
        // `header::from_one_raw_str` invokes `hv.to_str` which assumes `hv` contains only visible
        //  ASCII characters. So `hv.as_bytes` is necessary here.
        let hv = String::from_utf8(hv.as_bytes().to_vec())
            .map_err(|_| crate::error::ParseError::Header)?;
        let (disp_type, mut left) = split_once_and_trim(hv.as_str().trim(), ';');
        if disp_type.is_empty() {
            return Err(crate::error::ParseError::Header);
        }
        let mut cd = ContentDisposition {
            disposition: disp_type.into(),
            parameters: Vec::new(),
        };

        while !left.is_empty() {
            let (param_name, new_left) = split_once_and_trim(left, '=');
            if param_name.is_empty() || param_name == "*" || new_left.is_empty() {
                return Err(crate::error::ParseError::Header);
            }
            left = new_left;
            if let Some(param_name) = param_name.strip_suffix('*') {
                // extended parameters
                let (ext_value, new_left) = split_once_and_trim(left, ';');
                left = new_left;
                let ext_value = header::parse_extended_value(ext_value)?;

                let param = if param_name.eq_ignore_ascii_case("filename") {
                    DispositionParam::FilenameExt(ext_value)
                } else {
                    DispositionParam::UnknownExt(param_name.to_owned(), ext_value)
                };
                cd.parameters.push(param);
            } else {
                // regular parameters
                let value = if left.starts_with('\"') {
                    // quoted-string: defined in RFC6266 -> RFC2616 Section 3.6
                    let mut escaping = false;
                    let mut quoted_string = vec![];
                    let mut end = None;
                    // search for closing quote
                    for (i, &c) in left.as_bytes().iter().skip(1).enumerate() {
                        if escaping {
                            escaping = false;
                            quoted_string.push(c);
                        } else if c == 0x5c {
                            // backslash
                            escaping = true;
                        } else if c == 0x22 {
                            // double quote
                            end = Some(i + 1); // cuz skipped 1 for the leading quote
                            break;
                        } else {
                            quoted_string.push(c);
                        }
                    }
                    left = &left[end.ok_or(crate::error::ParseError::Header)? + 1..];
                    left = split_once(left, ';').1.trim_start();
                    // In fact, it should not be Err if the above code is correct.
                    String::from_utf8(quoted_string)
                        .map_err(|_| crate::error::ParseError::Header)?
                } else {
                    // token: won't contains semicolon according to RFC 2616 Section 2.2
                    let (token, new_left) = split_once_and_trim(left, ';');
                    left = new_left;
                    if token.is_empty() {
                        // quoted-string can be empty, but token cannot be empty
                        return Err(crate::error::ParseError::Header);
                    }
                    token.to_owned()
                };

                let param = if param_name.eq_ignore_ascii_case("name") {
                    DispositionParam::Name(value)
                } else if param_name.eq_ignore_ascii_case("filename") {
                    // See also comments in test_from_raw_unnecessary_percent_decode.
                    DispositionParam::Filename(value)
                } else {
                    DispositionParam::Unknown(param_name.to_owned(), value)
                };
                cd.parameters.push(param);
            }
        }

        Ok(cd)
    }

    /// Returns `true` if it is [`Inline`](DispositionType::Inline).
    pub fn is_inline(&self) -> bool {
        matches!(self.disposition, DispositionType::Inline)
    }

    /// Returns `true` if it is [`Attachment`](DispositionType::Attachment).
    pub fn is_attachment(&self) -> bool {
        matches!(self.disposition, DispositionType::Attachment)
    }

    /// Returns `true` if it is [`FormData`](DispositionType::FormData).
    pub fn is_form_data(&self) -> bool {
        matches!(self.disposition, DispositionType::FormData)
    }

    /// Returns `true` if it is [`Ext`](DispositionType::Ext) and the `disp_type` matches.
    pub fn is_ext<T: AsRef<str>>(&self, disp_type: T) -> bool {
        match self.disposition {
            DispositionType::Ext(ref t)
                if t.eq_ignore_ascii_case(disp_type.as_ref()) =>
            {
                true
            }
            _ => false,
        }
    }

    /// Return the value of *name* if exists.
    pub fn get_name(&self) -> Option<&str> {
        self.parameters.iter().filter_map(|p| p.as_name()).next()
    }

    /// Return the value of *filename* if exists.
    pub fn get_filename(&self) -> Option<&str> {
        self.parameters
            .iter()
            .filter_map(|p| p.as_filename())
            .next()
    }

    /// Return the value of *filename\** if exists.
    pub fn get_filename_ext(&self) -> Option<&ExtendedValue> {
        self.parameters
            .iter()
            .filter_map(|p| p.as_filename_ext())
            .next()
    }

    /// Return the value of the parameter which the `name` matches.
    pub fn get_unknown<T: AsRef<str>>(&self, name: T) -> Option<&str> {
        let name = name.as_ref();
        self.parameters
            .iter()
            .filter_map(|p| p.as_unknown(name))
            .next()
    }

    /// Return the value of the extended parameter which the `name` matches.
    pub fn get_unknown_ext<T: AsRef<str>>(&self, name: T) -> Option<&ExtendedValue> {
        let name = name.as_ref();
        self.parameters
            .iter()
            .filter_map(|p| p.as_unknown_ext(name))
            .next()
    }
}

impl IntoHeaderValue for ContentDisposition {
    type Error = header::InvalidHeaderValue;

    fn try_into_value(self) -> Result<header::HeaderValue, Self::Error> {
        let mut writer = Writer::new();
        let _ = write!(&mut writer, "{}", self);
        header::HeaderValue::from_maybe_shared(writer.take())
    }
}

impl Header for ContentDisposition {
    fn name() -> header::HeaderName {
        header::CONTENT_DISPOSITION
    }

    fn parse<T: crate::HttpMessage>(msg: &T) -> Result<Self, crate::error::ParseError> {
        if let Some(h) = msg.headers().get(&Self::name()) {
            Self::from_raw(&h)
        } else {
            Err(crate::error::ParseError::Header)
        }
    }
}

impl fmt::Display for DispositionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DispositionType::Inline => write!(f, "inline"),
            DispositionType::Attachment => write!(f, "attachment"),
            DispositionType::FormData => write!(f, "form-data"),
            DispositionType::Ext(ref s) => write!(f, "{}", s),
        }
    }
}

impl fmt::Display for DispositionParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // All ASCII control characters (0-30, 127) including horizontal tab, double quote, and
        // backslash should be escaped in quoted-string (i.e. "foobar").
        // Ref: RFC6266 S4.1 -> RFC2616 S3.6
        // filename-parm  = "filename" "=" value
        // value          = token | quoted-string
        // quoted-string  = ( <"> *(qdtext | quoted-pair ) <"> )
        // qdtext         = <any TEXT except <">>
        // quoted-pair    = "\" CHAR
        // TEXT           = <any OCTET except CTLs,
        //                  but including LWS>
        // LWS            = [CRLF] 1*( SP | HT )
        // OCTET          = <any 8-bit sequence of data>
        // CHAR           = <any US-ASCII character (octets 0 - 127)>
        // CTL            = <any US-ASCII control character
        //                  (octets 0 - 31) and DEL (127)>
        //
        // Ref: RFC7578 S4.2 -> RFC2183 S2 -> RFC2045 S5.1
        // parameter := attribute "=" value
        // attribute := token
        //              ; Matching of attributes
        //              ; is ALWAYS case-insensitive.
        // value := token / quoted-string
        // token := 1*<any (US-ASCII) CHAR except SPACE, CTLs,
        //             or tspecials>
        // tspecials :=  "(" / ")" / "<" / ">" / "@" /
        //               "," / ";" / ":" / "\" / <">
        //               "/" / "[" / "]" / "?" / "="
        //               ; Must be in quoted-string,
        //               ; to use within parameter values
        //
        //
        // See also comments in test_from_raw_unnecessary_percent_decode.
        static RE: Lazy<Regex> = Lazy::new(|| Regex::new("[\x00-\x08\x10-\x1F\x7F\"\\\\]").unwrap());
        match self {
            DispositionParam::Name(ref value) => write!(f, "name={}", value),
            DispositionParam::Filename(ref value) => {
                write!(f, "filename=\"{}\"", RE.replace_all(value, "\\$0").as_ref())
            }
            DispositionParam::Unknown(ref name, ref value) => write!(
                f,
                "{}=\"{}\"",
                name,
                &RE.replace_all(value, "\\$0").as_ref()
            ),
            DispositionParam::FilenameExt(ref ext_value) => {
                write!(f, "filename*={}", ext_value)
            }
            DispositionParam::UnknownExt(ref name, ref ext_value) => {
                write!(f, "{}*={}", name, ext_value)
            }
        }
    }
}

impl fmt::Display for ContentDisposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.disposition)?;
        self.parameters
            .iter()
            .try_for_each(|param| write!(f, "; {}", param))
    }
}

#[cfg(test)]
mod tests {
    use super::{ContentDisposition, DispositionParam, DispositionType};
    use crate::header::shared::Charset;
    use crate::header::{ExtendedValue, HeaderValue};

    #[test]
    fn test_from_raw_basic() {
        assert!(ContentDisposition::from_raw(&HeaderValue::from_static("")).is_err());

        let a = HeaderValue::from_static(
            "form-data; dummy=3; name=upload; filename=\"sample.png\"",
        );
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::FormData,
            parameters: vec![
                DispositionParam::Unknown("dummy".to_owned(), "3".to_owned()),
                DispositionParam::Name("upload".to_owned()),
                DispositionParam::Filename("sample.png".to_owned()),
            ],
        };
        assert_eq!(a, b);

        let a = HeaderValue::from_static("attachment; filename=\"image.jpg\"");
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::Attachment,
            parameters: vec![DispositionParam::Filename("image.jpg".to_owned())],
        };
        assert_eq!(a, b);

        let a = HeaderValue::from_static("inline; filename=image.jpg");
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::Inline,
            parameters: vec![DispositionParam::Filename("image.jpg".to_owned())],
        };
        assert_eq!(a, b);

        let a = HeaderValue::from_static(
            "attachment; creation-date=\"Wed, 12 Feb 1997 16:29:51 -0500\"",
        );
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::Attachment,
            parameters: vec![DispositionParam::Unknown(
                String::from("creation-date"),
                "Wed, 12 Feb 1997 16:29:51 -0500".to_owned(),
            )],
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_from_raw_extended() {
        let a = HeaderValue::from_static(
            "attachment; filename*=UTF-8''%c2%a3%20and%20%e2%82%ac%20rates",
        );
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::Attachment,
            parameters: vec![DispositionParam::FilenameExt(ExtendedValue {
                charset: Charset::Ext(String::from("UTF-8")),
                language_tag: None,
                value: vec![
                    0xc2, 0xa3, 0x20, b'a', b'n', b'd', 0x20, 0xe2, 0x82, 0xac, 0x20,
                    b'r', b'a', b't', b'e', b's',
                ],
            })],
        };
        assert_eq!(a, b);

        let a = HeaderValue::from_static(
            "attachment; filename*=UTF-8''%c2%a3%20and%20%e2%82%ac%20rates",
        );
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::Attachment,
            parameters: vec![DispositionParam::FilenameExt(ExtendedValue {
                charset: Charset::Ext(String::from("UTF-8")),
                language_tag: None,
                value: vec![
                    0xc2, 0xa3, 0x20, b'a', b'n', b'd', 0x20, 0xe2, 0x82, 0xac, 0x20,
                    b'r', b'a', b't', b'e', b's',
                ],
            })],
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_from_raw_extra_whitespace() {
        let a = HeaderValue::from_static(
            "form-data  ; du-mmy= 3  ; name =upload ; filename =  \"sample.png\"  ; ",
        );
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::FormData,
            parameters: vec![
                DispositionParam::Unknown("du-mmy".to_owned(), "3".to_owned()),
                DispositionParam::Name("upload".to_owned()),
                DispositionParam::Filename("sample.png".to_owned()),
            ],
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_from_raw_unordered() {
        let a = HeaderValue::from_static(
            "form-data; dummy=3; filename=\"sample.png\" ; name=upload;",
            // Actually, a trailing semicolon is not compliant. But it is fine to accept.
        );
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::FormData,
            parameters: vec![
                DispositionParam::Unknown("dummy".to_owned(), "3".to_owned()),
                DispositionParam::Filename("sample.png".to_owned()),
                DispositionParam::Name("upload".to_owned()),
            ],
        };
        assert_eq!(a, b);

        let a = HeaderValue::from_str(
            "attachment; filename*=iso-8859-1''foo-%E4.html; filename=\"foo-ä.html\"",
        )
        .unwrap();
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::Attachment,
            parameters: vec![
                DispositionParam::FilenameExt(ExtendedValue {
                    charset: Charset::Iso_8859_1,
                    language_tag: None,
                    value: b"foo-\xe4.html".to_vec(),
                }),
                DispositionParam::Filename("foo-ä.html".to_owned()),
            ],
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_from_raw_only_disp() {
        let a = ContentDisposition::from_raw(&HeaderValue::from_static("attachment"))
            .unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::Attachment,
            parameters: vec![],
        };
        assert_eq!(a, b);

        let a =
            ContentDisposition::from_raw(&HeaderValue::from_static("inline ;")).unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::Inline,
            parameters: vec![],
        };
        assert_eq!(a, b);

        let a = ContentDisposition::from_raw(&HeaderValue::from_static(
            "unknown-disp-param",
        ))
        .unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::Ext(String::from("unknown-disp-param")),
            parameters: vec![],
        };
        assert_eq!(a, b);
    }

    #[test]
    fn from_raw_with_mixed_case() {
        let a = HeaderValue::from_str(
            "InLInE; fIlenAME*=iso-8859-1''foo-%E4.html; filEName=\"foo-ä.html\"",
        )
        .unwrap();
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::Inline,
            parameters: vec![
                DispositionParam::FilenameExt(ExtendedValue {
                    charset: Charset::Iso_8859_1,
                    language_tag: None,
                    value: b"foo-\xe4.html".to_vec(),
                }),
                DispositionParam::Filename("foo-ä.html".to_owned()),
            ],
        };
        assert_eq!(a, b);
    }

    #[test]
    fn from_raw_with_unicode() {
        /* RFC7578 Section 4.2:
        Some commonly deployed systems use multipart/form-data with file names directly encoded
        including octets outside the US-ASCII range. The encoding used for the file names is
        typically UTF-8, although HTML forms will use the charset associated with the form.

        Mainstream browsers like Firefox (gecko) and Chrome use UTF-8 directly as above.
        (And now, only UTF-8 is handled by this implementation.)
        */
        let a = HeaderValue::from_str("form-data; name=upload; filename=\"文件.webp\"")
            .unwrap();
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::FormData,
            parameters: vec![
                DispositionParam::Name(String::from("upload")),
                DispositionParam::Filename(String::from("文件.webp")),
            ],
        };
        assert_eq!(a, b);

        let a = HeaderValue::from_str(
            "form-data; name=upload; filename=\"余固知謇謇之為患兮，忍而不能舍也.pptx\"",
        )
        .unwrap();
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::FormData,
            parameters: vec![
                DispositionParam::Name(String::from("upload")),
                DispositionParam::Filename(String::from(
                    "余固知謇謇之為患兮，忍而不能舍也.pptx",
                )),
            ],
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_from_raw_escape() {
        let a = HeaderValue::from_static(
            "form-data; dummy=3; name=upload; filename=\"s\\amp\\\"le.png\"",
        );
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::FormData,
            parameters: vec![
                DispositionParam::Unknown("dummy".to_owned(), "3".to_owned()),
                DispositionParam::Name("upload".to_owned()),
                DispositionParam::Filename(
                    ['s', 'a', 'm', 'p', '\"', 'l', 'e', '.', 'p', 'n', 'g']
                        .iter()
                        .collect(),
                ),
            ],
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_from_raw_semicolon() {
        let a =
            HeaderValue::from_static("form-data; filename=\"A semicolon here;.pdf\"");
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::FormData,
            parameters: vec![DispositionParam::Filename(String::from(
                "A semicolon here;.pdf",
            ))],
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_from_raw_unnecessary_percent_decode() {
        // In fact, RFC7578 (multipart/form-data) Section 2 and 4.2 suggests that filename with
        // non-ASCII characters MAY be percent-encoded.
        // On the contrary, RFC6266 or other RFCs related to Content-Disposition response header
        // do not mention such percent-encoding.
        // So, it appears to be undecidable whether to percent-decode or not without
        // knowing the usage scenario (multipart/form-data v.s. HTTP response header) and
        // inevitable to unnecessarily percent-decode filename with %XX in the former scenario.
        // Fortunately, it seems that almost all mainstream browsers just send UTF-8 encoded file
        // names in quoted-string format (tested on Edge, IE11, Chrome and Firefox) without
        // percent-encoding. So we do not bother to attempt to percent-decode.
        let a = HeaderValue::from_static(
            "form-data; name=photo; filename=\"%74%65%73%74%2e%70%6e%67\"",
        );
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::FormData,
            parameters: vec![
                DispositionParam::Name("photo".to_owned()),
                DispositionParam::Filename(String::from("%74%65%73%74%2e%70%6e%67")),
            ],
        };
        assert_eq!(a, b);

        let a = HeaderValue::from_static(
            "form-data; name=photo; filename=\"%74%65%73%74.png\"",
        );
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let b = ContentDisposition {
            disposition: DispositionType::FormData,
            parameters: vec![
                DispositionParam::Name("photo".to_owned()),
                DispositionParam::Filename(String::from("%74%65%73%74.png")),
            ],
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_from_raw_param_value_missing() {
        let a = HeaderValue::from_static("form-data; name=upload ; filename=");
        assert!(ContentDisposition::from_raw(&a).is_err());

        let a = HeaderValue::from_static("attachment; dummy=; filename=invoice.pdf");
        assert!(ContentDisposition::from_raw(&a).is_err());

        let a = HeaderValue::from_static("inline; filename=  ");
        assert!(ContentDisposition::from_raw(&a).is_err());

        let a = HeaderValue::from_static("inline; filename=\"\"");
        assert!(ContentDisposition::from_raw(&a)
            .expect("parse cd")
            .get_filename()
            .expect("filename")
            .is_empty());
    }

    #[test]
    fn test_from_raw_param_name_missing() {
        let a = HeaderValue::from_static("inline; =\"test.txt\"");
        assert!(ContentDisposition::from_raw(&a).is_err());

        let a = HeaderValue::from_static("inline; =diary.odt");
        assert!(ContentDisposition::from_raw(&a).is_err());

        let a = HeaderValue::from_static("inline; =");
        assert!(ContentDisposition::from_raw(&a).is_err());
    }

    #[test]
    fn test_display_extended() {
        let as_string =
            "attachment; filename*=UTF-8'en'%C2%A3%20and%20%E2%82%AC%20rates";
        let a = HeaderValue::from_static(as_string);
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let display_rendered = format!("{}", a);
        assert_eq!(as_string, display_rendered);

        let a = HeaderValue::from_static("attachment; filename=colourful.csv");
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let display_rendered = format!("{}", a);
        assert_eq!(
            "attachment; filename=\"colourful.csv\"".to_owned(),
            display_rendered
        );
    }

    #[test]
    fn test_display_quote() {
        let as_string = "form-data; name=upload; filename=\"Quote\\\"here.png\"";
        as_string
            .find(['\\', '\"'].iter().collect::<String>().as_str())
            .unwrap(); // ensure `\"` is there
        let a = HeaderValue::from_static(as_string);
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let display_rendered = format!("{}", a);
        assert_eq!(as_string, display_rendered);
    }

    #[test]
    fn test_display_space_tab() {
        let as_string = "form-data; name=upload; filename=\"Space here.png\"";
        let a = HeaderValue::from_static(as_string);
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let display_rendered = format!("{}", a);
        assert_eq!(as_string, display_rendered);

        let a: ContentDisposition = ContentDisposition {
            disposition: DispositionType::Inline,
            parameters: vec![DispositionParam::Filename(String::from("Tab\there.png"))],
        };
        let display_rendered = format!("{}", a);
        assert_eq!("inline; filename=\"Tab\x09here.png\"", display_rendered);
    }

    #[test]
    fn test_display_control_characters() {
        /* let a = "attachment; filename=\"carriage\rreturn.png\"";
        let a = HeaderValue::from_static(a);
        let a: ContentDisposition = ContentDisposition::from_raw(&a).unwrap();
        let display_rendered = format!("{}", a);
        assert_eq!(
            "attachment; filename=\"carriage\\\rreturn.png\"",
            display_rendered
        );*/
        // No way to create a HeaderValue containing a carriage return.

        let a: ContentDisposition = ContentDisposition {
            disposition: DispositionType::Inline,
            parameters: vec![DispositionParam::Filename(String::from("bell\x07.png"))],
        };
        let display_rendered = format!("{}", a);
        assert_eq!("inline; filename=\"bell\\\x07.png\"", display_rendered);
    }

    #[test]
    fn test_param_methods() {
        let param = DispositionParam::Filename(String::from("sample.txt"));
        assert!(param.is_filename());
        assert_eq!(param.as_filename().unwrap(), "sample.txt");

        let param = DispositionParam::Unknown(String::from("foo"), String::from("bar"));
        assert!(param.is_unknown("foo"));
        assert_eq!(param.as_unknown("fOo"), Some("bar"));
    }

    #[test]
    fn test_disposition_methods() {
        let cd = ContentDisposition {
            disposition: DispositionType::FormData,
            parameters: vec![
                DispositionParam::Unknown("dummy".to_owned(), "3".to_owned()),
                DispositionParam::Name("upload".to_owned()),
                DispositionParam::Filename("sample.png".to_owned()),
            ],
        };
        assert_eq!(cd.get_name(), Some("upload"));
        assert_eq!(cd.get_unknown("dummy"), Some("3"));
        assert_eq!(cd.get_filename(), Some("sample.png"));
        assert_eq!(cd.get_unknown_ext("dummy"), None);
        assert_eq!(cd.get_unknown("duMMy"), Some("3"));
    }
}
