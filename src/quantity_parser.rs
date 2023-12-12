// Modified version of https://github.com/sombralibre/k8s-quantity-parser
//
// Original license:
//
// MIT License
//
// Copyright (c) 2022 Alejandro Llanes
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use color_eyre::{eyre::eyre, Report, Result};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use regex::Regex;

#[allow(non_camel_case_types)]
enum QuantityMemoryUnits {
    Ki,
    Mi,
    Gi,
    Ti,
    Pi,
    Ei,
    k,
    M,
    G,
    T,
    P,
    E,
    m,
    Invalid,
}

impl QuantityMemoryUnits {
    fn new(unit: &str) -> Self {
        match unit {
            "Ki" => Self::Ki,
            "Mi" => Self::Mi,
            "Gi" => Self::Gi,
            "Ti" => Self::Ti,
            "Pi" => Self::Pi,
            "Ei" => Self::Ei,
            "k" => Self::k,
            "M" => Self::M,
            "G" => Self::G,
            "T" => Self::T,
            "P" => Self::P,
            "E" => Self::E,
            "m" => Self::m,
            _ => Self::Invalid,
        }
    }
}

/// This trait works as a parser for the values retrieved from BTreeMap<String, Quantity> collections
/// in `k8s_openapi::api::core::v1::Pod` and `k8s_openapi::api::core::v1::Node`
///
/// # Errors
/// The parser will fails if encounters an invalid unit letters or failed to parse String to i64

pub trait QuantityParser {
    /// This method will parse the cpu resource values returned by Kubernetes Api
    ///
    /// ```rust
    /// # use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
    /// # use k8s_quantity_parser::QuantityParser;
    /// #
    /// let mib = Quantity("1Mi".into());
    /// let ret: i64 = 1048576;
    /// assert_eq!(mib.to_bytes().ok().flatten().unwrap(), ret);
    /// ```
    ///
    /// # Errors
    ///
    /// The parser will fails if encounters an invalid unit letters or failed to parse String to i64
    ///
    fn to_milli_cpus(&self) -> Result<Option<i64>, Report>;
    /// This method will parse the memory resource values returned by Kubernetes Api
    ///
    /// ```rust
    /// # use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
    /// # use k8s_quantity_parser::QuantityParser;
    /// #
    /// let cpu = Quantity("4".into());
    /// let ret: i64 = 4000;
    /// assert_eq!(cpu.to_milli_cpus().ok().flatten().unwrap(), ret)
    /// ```
    ///
    /// # Errors
    ///
    /// The parser will fails if encounters an invalid unit letters or failed to parse String to i64
    ///
    fn to_bytes(&self) -> Result<Option<i64>, Report>;
}

impl QuantityParser for Quantity {
    fn to_milli_cpus(&self) -> Result<Option<i64>, Report> {
        let unit_str = &self.0;
        let rgx = Regex::new(r"([m]{1}$)")?;
        let cap = rgx.captures(unit_str);
        if cap.is_none() {
            return Ok(Some(unit_str.parse::<i64>()? * 1000));
        };
        let mt = cap.unwrap().get(0).unwrap();
        let unit_str = unit_str.replace(mt.as_str(), "");
        Ok(Some(unit_str.parse::<i64>()?))
    }

    fn to_bytes(&self) -> Result<Option<i64>, Report> {
        let unit_str = &self.0;
        let rgx = Regex::new(r"([[:alpha:]]{1,2}$)")?;
        let cap = rgx.captures(unit_str);

        if cap.is_none() {
            return Ok(Some(unit_str.parse::<i64>()?));
        };

        // Is safe to use unwrap here, as the value is already checked.
        match cap.unwrap().get(0) {
            Some(m) => match QuantityMemoryUnits::new(m.as_str()) {
                QuantityMemoryUnits::Ki => {
                    let unit_str = unit_str.replace(m.as_str(), "");
                    let amount = unit_str.parse::<i64>()?;
                    Ok(Some(amount * 1024))
                }
                QuantityMemoryUnits::Mi => {
                    let unit_str = unit_str.replace(m.as_str(), "");
                    let amount = unit_str.parse::<i64>()?;
                    Ok(Some((amount * 1024) * 1024))
                }
                QuantityMemoryUnits::Gi => {
                    let unit_str = unit_str.replace(m.as_str(), "");
                    let amount = unit_str.parse::<i64>()?;
                    Ok(Some(((amount * 1024) * 1024) * 1024))
                }
                QuantityMemoryUnits::Ti => {
                    let unit_str = unit_str.replace(m.as_str(), "");
                    let amount = unit_str.parse::<i64>()?;
                    Ok(Some((((amount * 1024) * 1024) * 1024) * 1024))
                }
                QuantityMemoryUnits::Pi => {
                    let unit_str = unit_str.replace(m.as_str(), "");
                    let amount = unit_str.parse::<i64>()?;
                    Ok(Some(((((amount * 1024) * 1024) * 1024) * 1024) * 1024))
                }
                QuantityMemoryUnits::Ei => {
                    let unit_str = unit_str.replace(m.as_str(), "");
                    let amount = unit_str.parse::<i64>()?;
                    Ok(Some(
                        (((((amount * 1024) * 1024) * 1024) * 1024) * 1024) * 1024,
                    ))
                }
                QuantityMemoryUnits::k => {
                    let unit_str = unit_str.replace(m.as_str(), "");
                    let amount = unit_str.parse::<i64>()?;
                    Ok(Some(amount * 1000))
                }
                QuantityMemoryUnits::M => {
                    let unit_str = unit_str.replace(m.as_str(), "");
                    let amount = unit_str.parse::<i64>()?;
                    Ok(Some((amount * 1000) * 1000))
                }
                QuantityMemoryUnits::G => {
                    let unit_str = unit_str.replace(m.as_str(), "");
                    let amount = unit_str.parse::<i64>()?;
                    Ok(Some(((amount * 1000) * 1000) * 1000))
                }
                QuantityMemoryUnits::T => {
                    let unit_str = unit_str.replace(m.as_str(), "");
                    let amount = unit_str.parse::<i64>()?;
                    Ok(Some((((amount * 1000) * 1000) * 1000) * 1000))
                }
                QuantityMemoryUnits::P => {
                    let unit_str = unit_str.replace(m.as_str(), "");
                    let amount = unit_str.parse::<i64>()?;
                    Ok(Some(((((amount * 1000) * 1000) * 1000) * 1000) * 1000))
                }
                QuantityMemoryUnits::E => {
                    let unit_str = unit_str.replace(m.as_str(), "");
                    let amount = unit_str.parse::<i64>()?;
                    Ok(Some(
                        (((((amount * 1000) * 1000) * 1000) * 1000) * 1000) * 1000,
                    ))
                }
                QuantityMemoryUnits::m => {
                    let unit_str = unit_str.replace(m.as_str(), "");
                    let amount = unit_str.parse::<i64>()?;
                    Ok(Some(amount / 1000))
                }
                QuantityMemoryUnits::Invalid => Err(eyre!("Invalid unit")),
            },
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_bytes_works() {
        assert!(Quantity("12345".into()).to_bytes().is_ok())
    }

    #[test]
    fn to_bytes_is_some() {
        assert!(Quantity("12345".into()).to_bytes().unwrap().is_some())
    }

    #[test]
    fn to_milli_cpus_works() {
        assert!(Quantity("12345m".into()).to_milli_cpus().is_ok())
    }

    #[test]
    fn to_milli_cpus_is_some() {
        assert!(Quantity("12345m".into()).to_milli_cpus().unwrap().is_some())
    }

    #[test]
    fn invalid_unit_fails() {
        assert!(Quantity("12345r".into()).to_bytes().is_err())
    }

    #[test]
    fn parse_i64_fails() {
        assert!(Quantity("123.123".into()).to_bytes().is_err())
    }

    #[test]
    fn is_none_value() {
        assert!(Quantity("0Mi".into()).to_bytes().unwrap().is_some())
    }

    #[test]
    fn pow2_mb_to_bytes() {
        let mib = Quantity("1Mi".into());
        let ret: i64 = 1048576;
        assert_eq!(mib.to_bytes().ok().flatten().unwrap(), ret);
    }

    #[test]
    fn pow10_gb_to_bytes() {
        let mib = Quantity("1G".into());
        let ret: i64 = 1000000000;
        assert_eq!(mib.to_bytes().ok().flatten().unwrap(), ret);
    }

    #[test]
    fn cpu_units_value_to_millis() {
        let cpu = Quantity("1536m".into());
        let ret: i64 = 1536;
        assert_eq!(cpu.to_milli_cpus().ok().flatten().unwrap(), ret)
    }

    #[test]
    fn cpu_cores_value_to_millis() {
        let cpu = Quantity("4".into());
        let ret: i64 = 4000;
        assert_eq!(cpu.to_milli_cpus().ok().flatten().unwrap(), ret)
    }
}
