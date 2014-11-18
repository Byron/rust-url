// Copyright 2013-2014 Simon Sapin.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

pub use Host::{Domain,Ipv6};

use std::ascii::OwnedAsciiExt;
use std::cmp;
use std::fmt::{Formatter, FormatError, Show};
use parser::{
    ParseResult,
    InvalidIpv6Address, EmptyHost, NonAsciiDomainsNotSupportedYet, InvalidDomainCharacter,
};
use percent_encoding::{from_hex, percent_decode};


/// The host name of an URL.
#[deriving(PartialEq, Eq, Clone)]
pub enum Host {
    /// A (DNS) domain name or an IPv4 address.
    ///
    /// FIXME: IPv4 probably should be a separate variant.
    /// See https://www.w3.org/Bugs/Public/show_bug.cgi?id=26431
    Domain(String),

    /// An IPv6 address, represented inside `[...]` square brackets
    /// so that `:` colon characters in the address are not ambiguous
    /// with the port number delimiter.
    Ipv6(Ipv6Address),
}


/// A 128 bit IPv6 address
pub struct Ipv6Address {
    pub pieces: [u16, ..8]
}

impl Clone for Ipv6Address {
    fn clone(&self) -> Ipv6Address {
        Ipv6Address { pieces: self.pieces }
    }
}

impl Eq for Ipv6Address {}

impl PartialEq for Ipv6Address {
    fn eq(&self, other: &Ipv6Address) -> bool {
        self.pieces == other.pieces
    }
}


impl Host {
    /// Parse a host: either an IPv6 address in [] square brackets, or a domain.
    ///
    /// Returns `Err` for an empty host, an invalid IPv6 address,
    /// or a or invalid non-ASCII domain.
    ///
    /// FIXME: Add IDNA support for non-ASCII domains.
    pub fn parse(input: &str) -> ParseResult<Host> {
        if input.len() == 0 {
            Err(EmptyHost)
        } else if input.starts_with("[") {
            if input.ends_with("]") {
                Ipv6Address::parse(input.slice(1, input.len() - 1)).map(Ipv6)
            } else {
                Err(InvalidIpv6Address)
            }
        } else {
            let decoded = percent_decode(input.as_bytes());
            let domain = String::from_utf8_lossy(decoded.as_slice());
            // TODO: Remove this check and use IDNA "domain to ASCII"
            if !domain.as_slice().is_ascii() {
                Err(NonAsciiDomainsNotSupportedYet)
            } else if domain.as_slice().find([
                '\0', '\t', '\n', '\r', ' ', '#', '%', '/', ':', '?', '@', '[', '\\', ']'
            ].as_slice()).is_some() {
                Err(InvalidDomainCharacter)
            } else {
                Ok(Domain(domain.into_string().into_ascii_lower()))
            }
        }
    }

    /// Serialize the host as a string.
    ///
    /// A domain a returned as-is, an IPv6 address between [] square brackets.
    pub fn serialize(&self) -> String {
        self.to_string()
    }
}


impl Show for Host {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), FormatError> {
        match *self {
            Domain(ref domain) => domain.fmt(formatter),
            Ipv6(ref address) => {
                try!(formatter.write(b"["));
                try!(address.fmt(formatter));
                formatter.write(b"]")
            }
        }
    }
}


impl Ipv6Address {
    /// Parse an IPv6 address, without the [] square brackets.
    pub fn parse(input: &str) -> ParseResult<Ipv6Address> {
        let input = input.as_bytes();
        let len = input.len();
        let mut is_ip_v4 = false;
        let mut pieces = [0, 0, 0, 0, 0, 0, 0, 0];
        let mut piece_pointer = 0u;
        let mut compress_pointer = None;
        let mut i = 0u;
        if input[0] == b':' {
            if input[1] != b':' {
                return Err(InvalidIpv6Address)
            }
            i = 2;
            piece_pointer = 1;
            compress_pointer = Some(1u);
        }

        while i < len {
            if piece_pointer == 8 {
                return Err(InvalidIpv6Address)
            }
            if input[i] == b':' {
                if compress_pointer.is_some() {
                    return Err(InvalidIpv6Address)
                }
                i += 1;
                piece_pointer += 1;
                compress_pointer = Some(piece_pointer);
                continue
            }
            let start = i;
            let end = cmp::min(len, start + 4);
            let mut value = 0u16;
            while i < end {
                match from_hex(input[i]) {
                    Some(digit) => {
                        value = value * 0x10 + digit as u16;
                        i += 1;
                    },
                    None => break
                }
            }
            if i < len {
                match input[i] {
                    b'.' => {
                        if i == start {
                            return Err(InvalidIpv6Address)
                        }
                        i = start;
                        is_ip_v4 = true;
                    },
                    b':' => {
                        i += 1;
                        if i == len {
                            return Err(InvalidIpv6Address)
                        }
                    },
                    _ => return Err(InvalidIpv6Address)
                }
            }
            if is_ip_v4 {
                break
            }
            pieces[piece_pointer] = value;
            piece_pointer += 1;
        }

        if is_ip_v4 {
            if piece_pointer > 6 {
                return Err(InvalidIpv6Address)
            }
            let mut dots_seen = 0u;
            while i < len {
                // FIXME: https://github.com/whatwg/url/commit/1c22aa119c354e0020117e02571cec53f7c01064
                let mut value = 0u16;
                while i < len {
                    let digit = match input[i] {
                        c @ b'0' ... b'9' => c - b'0',
                        _ => break
                    };
                    value = value * 10 + digit as u16;
                    if value == 0 || value > 255 {
                        return Err(InvalidIpv6Address)
                    }
                }
                if dots_seen < 3 && !(i < len && input[i] == b'.') {
                    return Err(InvalidIpv6Address)
                }
                pieces[piece_pointer] = pieces[piece_pointer] * 0x100 + value;
                if dots_seen == 0 || dots_seen == 2 {
                    piece_pointer += 1;
                }
                i += 1;
                if dots_seen == 3 && i < len {
                    return Err(InvalidIpv6Address)
                }
                dots_seen += 1;
            }
        }

        match compress_pointer {
            Some(compress_pointer) => {
                let mut swaps = piece_pointer - compress_pointer;
                piece_pointer = 7;
                while swaps > 0 {
                    pieces[piece_pointer] = pieces[compress_pointer + swaps - 1];
                    pieces[compress_pointer + swaps - 1] = 0;
                    swaps -= 1;
                    piece_pointer -= 1;
                }
            }
            _ => if piece_pointer != 8 {
                return Err(InvalidIpv6Address)
            }
        }
        Ok(Ipv6Address { pieces: pieces })
    }

    /// Serialize the IPv6 address to a string.
    pub fn serialize(&self) -> String {
        self.to_string()
    }
}


impl Show for Ipv6Address {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), FormatError> {
        let (compress_start, compress_end) = longest_zero_sequence(&self.pieces);
        let mut i = 0;
        while i < 8 {
            if i == compress_start {
                try!(formatter.write(b":"));
                if i == 0 {
                    try!(formatter.write(b":"));
                }
                if compress_end < 8 {
                    i = compress_end;
                } else {
                    break;
                }
            }
            try!(write!(formatter, "{:x}", self.pieces[i as uint]));
            if i < 7 {
                try!(formatter.write(b":"));
            }
            i += 1;
        }
        Ok(())
    }
}


fn longest_zero_sequence(pieces: &[u16, ..8]) -> (int, int) {
    let mut longest = -1;
    let mut longest_length = -1;
    let mut start = -1;
    macro_rules! finish_sequence(
        ($end: expr) => {
            if start >= 0 {
                let length = $end - start;
                if length > longest_length {
                    longest = start;
                    longest_length = length;
                }
            }
        };
    );
    for i in range(0, 8) {
        if pieces[i as uint] == 0 {
            if start < 0 {
                start = i;
            }
        } else {
            finish_sequence!(i);
            start = -1;
        }
    }
    finish_sequence!(8);
    (longest, longest + longest_length)
}