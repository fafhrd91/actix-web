use std::fmt::{self, Display, Write};
use std::str::FromStr;

use super::{
    from_one_raw_str, Header, HeaderName, HeaderValue, IntoHeaderValue, InvalidHeaderValue,
    Writer,
};
use actix_http::error::ParseError;
use actix_http::header;
use actix_http::HttpMessage;

/// `Range` header, defined in [RFC7233](https://tools.ietf.org/html/rfc7233#section-3.1)
///
/// The "Range" header field on a GET request modifies the method
/// semantics to request transfer of only one or more sub-ranges of the
/// selected representation data, rather than the entire selected
/// representation data.
///
/// # ABNF
///
/// ```text
/// Range =	byte-ranges-specifier / other-ranges-specifier
/// other-ranges-specifier = other-range-unit "=" other-range-set
/// other-range-set = 1*VCHAR
///
/// bytes-unit = "bytes"
///
/// byte-ranges-specifier = bytes-unit "=" byte-range-set
/// byte-range-set = 1#(byte-range-spec / suffix-byte-range-spec)
/// byte-range-spec = first-byte-pos "-" [last-byte-pos]
/// first-byte-pos = 1*DIGIT
/// last-byte-pos = 1*DIGIT
/// ```
///
/// # Example values
///
/// * `bytes=1000-`
/// * `bytes=-2000`
/// * `bytes=0-1,30-40`
/// * `bytes=0-10,20-90,-100`
/// * `custom_unit=0-123`
/// * `custom_unit=xxx-yyy`
///
/// # Examples
///
/// ```
/// use actix_web::http::header::{Range, ByteRangeSpec};
/// use actix_web::HttpResponse;
///
/// let mut builder = HttpResponse::Ok();
/// builder.insert_header(Range::Bytes(
///     vec![ByteRangeSpec::FromTo(1, 100), ByteRangeSpec::AllFrom(200)]
/// ));
/// builder.insert_header(
///     Range::Unregistered("letters".to_owned(), "a-f".to_owned())
/// );
/// builder.insert_header(
///     Range::bytes(1, 100)
/// );
/// builder.insert_header(
///     Range::bytes_multi(vec![(1, 100), (200, 300)])
/// );
/// ```
#[derive(PartialEq, Clone, Debug)]
pub enum Range {
    /// Byte range
    Bytes(Vec<ByteRangeSpec>),
    /// Custom range, with unit not registered at IANA
    /// (`other-range-unit`: String , `other-range-set`: String)
    Unregistered(String, String),
}

/// Each `Range::Bytes` header can contain one or more `ByteRangeSpecs`.
/// Each `ByteRangeSpec` defines a range of bytes to fetch
#[derive(PartialEq, Clone, Debug)]
pub enum ByteRangeSpec {
    /// Get all bytes between x and y ("x-y")
    FromTo(u64, u64),
    /// Get all bytes starting from x ("x-")
    AllFrom(u64),
    /// Get last x bytes ("-x")
    Last(u64),
}

impl ByteRangeSpec {
    /// Given the full length of the entity, attempt to normalize the byte range
    /// into an satisfiable end-inclusive (from, to) range.
    ///
    /// The resulting range is guaranteed to be a satisfiable range within the
    /// bounds of `0 <= from <= to < full_length`.
    ///
    /// If the byte range is deemed unsatisfiable, `None` is returned.
    /// An unsatisfiable range is generally cause for a server to either reject
    /// the client request with a `416 Range Not Satisfiable` status code, or to
    /// simply ignore the range header and serve the full entity using a `200
    /// OK` status code.
    ///
    /// This function closely follows [RFC 7233][1] section 2.1.
    /// As such, it considers ranges to be satisfiable if they meet the
    /// following conditions:
    ///
    /// > If a valid byte-range-set includes at least one byte-range-spec with
    /// a first-byte-pos that is less than the current length of the
    /// representation, or at least one suffix-byte-range-spec with a
    /// non-zero suffix-length, then the byte-range-set is satisfiable.
    /// Otherwise, the byte-range-set is unsatisfiable.
    ///
    /// The function also computes remainder ranges based on the RFC:
    ///
    /// > If the last-byte-pos value is
    /// absent, or if the value is greater than or equal to the current
    /// length of the representation data, the byte range is interpreted as
    /// the remainder of the representation (i.e., the server replaces the
    /// value of last-byte-pos with a value that is one less than the current
    /// length of the selected representation).
    ///
    /// [1]: https://tools.ietf.org/html/rfc7233
    pub fn to_satisfiable_range(&self, full_length: u64) -> Option<(u64, u64)> {
        // If the full length is zero, there is no satisfiable end-inclusive range.
        if full_length == 0 {
            return None;
        }
        match self {
            &ByteRangeSpec::FromTo(from, to) => {
                if from < full_length && from <= to {
                    Some((from, ::std::cmp::min(to, full_length - 1)))
                } else {
                    None
                }
            }
            &ByteRangeSpec::AllFrom(from) => {
                if from < full_length {
                    Some((from, full_length - 1))
                } else {
                    None
                }
            }
            &ByteRangeSpec::Last(last) => {
                if last > 0 {
                    // From the RFC: If the selected representation is shorter
                    // than the specified suffix-length,
                    // the entire representation is used.
                    if last > full_length {
                        Some((0, full_length - 1))
                    } else {
                        Some((full_length - last, full_length - 1))
                    }
                } else {
                    None
                }
            }
        }
    }
}

impl Range {
    /// Get the most common byte range header ("bytes=from-to")
    pub fn bytes(from: u64, to: u64) -> Range {
        Range::Bytes(vec![ByteRangeSpec::FromTo(from, to)])
    }

    /// Get byte range header with multiple subranges
    /// ("bytes=from1-to1,from2-to2,fromX-toX")
    pub fn bytes_multi(ranges: Vec<(u64, u64)>) -> Range {
        Range::Bytes(
            ranges
                .iter()
                .map(|r| ByteRangeSpec::FromTo(r.0, r.1))
                .collect(),
        )
    }
}

impl fmt::Display for ByteRangeSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ByteRangeSpec::FromTo(from, to) => write!(f, "{}-{}", from, to),
            ByteRangeSpec::Last(pos) => write!(f, "-{}", pos),
            ByteRangeSpec::AllFrom(pos) => write!(f, "{}-", pos),
        }
    }
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Range::Bytes(ref ranges) => {
                write!(f, "bytes=")?;

                for (i, range) in ranges.iter().enumerate() {
                    if i != 0 {
                        f.write_str(",")?;
                    }
                    Display::fmt(range, f)?;
                }
                Ok(())
            }
            Range::Unregistered(ref unit, ref range_str) => {
                write!(f, "{}={}", unit, range_str)
            }
        }
    }
}

impl FromStr for Range {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Range, ParseError> {
        let mut iter = s.splitn(2, '=');

        match (iter.next(), iter.next()) {
            (Some("bytes"), Some(ranges)) => {
                let ranges = from_comma_delimited(ranges);
                if ranges.is_empty() {
                    return Err(ParseError::Header);
                }
                Ok(Range::Bytes(ranges))
            }
            (Some(unit), Some(range_str)) if unit != "" && range_str != "" => {
                Ok(Range::Unregistered(unit.to_owned(), range_str.to_owned()))
            }
            _ => Err(ParseError::Header),
        }
    }
}

impl FromStr for ByteRangeSpec {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<ByteRangeSpec, ParseError> {
        let mut parts = s.splitn(2, '-');

        match (parts.next(), parts.next()) {
            (Some(""), Some(end)) => end
                .parse()
                .or(Err(ParseError::Header))
                .map(ByteRangeSpec::Last),
            (Some(start), Some("")) => start
                .parse()
                .or(Err(ParseError::Header))
                .map(ByteRangeSpec::AllFrom),
            (Some(start), Some(end)) => match (start.parse(), end.parse()) {
                (Ok(start), Ok(end)) if start <= end => Ok(ByteRangeSpec::FromTo(start, end)),
                _ => Err(ParseError::Header),
            },
            _ => Err(ParseError::Header),
        }
    }
}

fn from_comma_delimited<T: FromStr>(s: &str) -> Vec<T> {
    s.split(',')
        .filter_map(|x| match x.trim() {
            "" => None,
            y => Some(y),
        })
        .filter_map(|x| x.parse().ok())
        .collect()
}

impl Header for Range {
    fn name() -> HeaderName {
        header::RANGE
    }

    #[inline]
    fn parse<T: HttpMessage>(msg: &T) -> Result<Self, ParseError> {
        from_one_raw_str(msg.headers().get(&header::RANGE))
    }
}

impl IntoHeaderValue for Range {
    type Error = InvalidHeaderValue;

    fn try_into_value(self) -> Result<HeaderValue, Self::Error> {
        let mut writer = Writer::new();
        let _ = write!(&mut writer, "{}", self);
        HeaderValue::from_maybe_shared(writer.take())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_http::test::TestRequest;
    use actix_http::Request;

    fn req(s: &str) -> Request {
        TestRequest::default()
            .insert_header((header::RANGE, s))
            .finish()
    }

    #[test]
    fn test_parse_bytes_range_valid() {
        let r: Range = Header::parse(&req("bytes=1-100")).unwrap();
        let r2: Range = Header::parse(&req("bytes=1-100,-")).unwrap();
        let r3 = Range::bytes(1, 100);
        assert_eq!(r, r2);
        assert_eq!(r2, r3);

        let r: Range = Header::parse(&req("bytes=1-100,200-")).unwrap();
        let r2: Range = Header::parse(&req("bytes= 1-100 , 101-xxx,  200- ")).unwrap();
        let r3 = Range::Bytes(vec![
            ByteRangeSpec::FromTo(1, 100),
            ByteRangeSpec::AllFrom(200),
        ]);
        assert_eq!(r, r2);
        assert_eq!(r2, r3);

        let r: Range = Header::parse(&req("bytes=1-100,-100")).unwrap();
        let r2: Range = Header::parse(&req("bytes=1-100, ,,-100")).unwrap();
        let r3 = Range::Bytes(vec![
            ByteRangeSpec::FromTo(1, 100),
            ByteRangeSpec::Last(100),
        ]);
        assert_eq!(r, r2);
        assert_eq!(r2, r3);

        let r: Range = Header::parse(&req("custom=1-100,-100")).unwrap();
        let r2 = Range::Unregistered("custom".to_owned(), "1-100,-100".to_owned());
        assert_eq!(r, r2);
    }

    #[test]
    fn test_parse_unregistered_range_valid() {
        let r: Range = Header::parse(&req("custom=1-100,-100")).unwrap();
        let r2 = Range::Unregistered("custom".to_owned(), "1-100,-100".to_owned());
        assert_eq!(r, r2);

        let r: Range = Header::parse(&req("custom=abcd")).unwrap();
        let r2 = Range::Unregistered("custom".to_owned(), "abcd".to_owned());
        assert_eq!(r, r2);

        let r: Range = Header::parse(&req("custom=xxx-yyy")).unwrap();
        let r2 = Range::Unregistered("custom".to_owned(), "xxx-yyy".to_owned());
        assert_eq!(r, r2);
    }

    #[test]
    fn test_parse_invalid() {
        let r: Result<Range, ParseError> = Header::parse(&req("bytes=1-a,-"));
        assert_eq!(r.ok(), None);

        let r: Result<Range, ParseError> = Header::parse(&req("bytes=1-2-3"));
        assert_eq!(r.ok(), None);

        let r: Result<Range, ParseError> = Header::parse(&req("abc"));
        assert_eq!(r.ok(), None);

        let r: Result<Range, ParseError> = Header::parse(&req("bytes=1-100="));
        assert_eq!(r.ok(), None);

        let r: Result<Range, ParseError> = Header::parse(&req("bytes="));
        assert_eq!(r.ok(), None);

        let r: Result<Range, ParseError> = Header::parse(&req("custom="));
        assert_eq!(r.ok(), None);

        let r: Result<Range, ParseError> = Header::parse(&req("=1-100"));
        assert_eq!(r.ok(), None);
    }

    #[test]
    fn test_fmt() {
        let range = Range::Bytes(vec![
            ByteRangeSpec::FromTo(0, 1000),
            ByteRangeSpec::AllFrom(2000),
        ]);
        assert_eq!(&range.to_string(), "bytes=0-1000,2000-");

        let range = Range::Bytes(vec![]);

        assert_eq!(&range.to_string(), "bytes=");

        let range = Range::Unregistered("custom".to_owned(), "1-xxx".to_owned());

        assert_eq!(&range.to_string(), "custom=1-xxx");
    }

    #[test]
    fn test_byte_range_spec_to_satisfiable_range() {
        assert_eq!(
            Some((0, 0)),
            ByteRangeSpec::FromTo(0, 0).to_satisfiable_range(3)
        );
        assert_eq!(
            Some((1, 2)),
            ByteRangeSpec::FromTo(1, 2).to_satisfiable_range(3)
        );
        assert_eq!(
            Some((1, 2)),
            ByteRangeSpec::FromTo(1, 5).to_satisfiable_range(3)
        );
        assert_eq!(None, ByteRangeSpec::FromTo(3, 3).to_satisfiable_range(3));
        assert_eq!(None, ByteRangeSpec::FromTo(2, 1).to_satisfiable_range(3));
        assert_eq!(None, ByteRangeSpec::FromTo(0, 0).to_satisfiable_range(0));

        assert_eq!(
            Some((0, 2)),
            ByteRangeSpec::AllFrom(0).to_satisfiable_range(3)
        );
        assert_eq!(
            Some((2, 2)),
            ByteRangeSpec::AllFrom(2).to_satisfiable_range(3)
        );
        assert_eq!(None, ByteRangeSpec::AllFrom(3).to_satisfiable_range(3));
        assert_eq!(None, ByteRangeSpec::AllFrom(5).to_satisfiable_range(3));
        assert_eq!(None, ByteRangeSpec::AllFrom(0).to_satisfiable_range(0));

        assert_eq!(Some((1, 2)), ByteRangeSpec::Last(2).to_satisfiable_range(3));
        assert_eq!(Some((2, 2)), ByteRangeSpec::Last(1).to_satisfiable_range(3));
        assert_eq!(Some((0, 2)), ByteRangeSpec::Last(5).to_satisfiable_range(3));
        assert_eq!(None, ByteRangeSpec::Last(0).to_satisfiable_range(3));
        assert_eq!(None, ByteRangeSpec::Last(2).to_satisfiable_range(0));
    }
}
