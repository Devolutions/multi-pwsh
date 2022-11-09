#![allow(dead_code)]

use crate::time::parse_iso8601_duration;
use crate::time::DateTime;
use quick_xml::events;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::time::Duration;
use url::Url;
use uuid::Uuid;

// [MS-PSRP]: PowerShell Remoting Protocol
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp

// [XMLSCHEMA2]: XML Schema Part 2: Datatypes Second Edition
// https://www.w3.org/TR/xmlschema-2/

// Serialization of Primitive Type Objects:
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/c8c85974-ffd7-4455-84a8-e49016c20683

// Serialization of Complex Objects:
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/406ad572-1ede-43e0-b063-e7291cda3e63

// Object type (<Obj>)
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/3e107e78-3f28-4f85-9e25-493fd9b09726

#[derive(Debug, Clone, Default)]
pub struct CliObject {
    pub name: Option<String>,
    pub values: Vec<CliValue>,
    pub ref_id: Option<String>,
    pub type_names: Vec<String>,
    pub string_repr: Option<String>,
}

impl CliObject {
    pub fn new(
        name: Option<&str>,
        value: Vec<CliValue>,
        ref_id: Option<&str>,
        type_names: Vec<String>,
        string_repr: Option<&str>,
    ) -> CliObject {
        CliObject {
            name: name.map(|s| s.to_string()),
            values: value,
            ref_id: ref_id.map(|s| s.to_string()),
            type_names: type_names,
            string_repr: string_repr.map(|s| s.to_string()),
        }
    }
}

// Type Names (<TN>, <T>, <TNRef>)
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/2784bd9c-267d-4297-b603-722c727f85f1

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliTypeName {
    pub name: Option<String>,
    pub values: Vec<String>,
    pub ref_id: String,
}

impl CliTypeName {
    pub fn new(name: Option<&str>, value: Vec<String>, ref_id: &str) -> CliTypeName {
        CliTypeName {
            name: name.map(|s| s.to_string()),
            values: value,
            ref_id: ref_id.to_string(),
        }
    }
}

// String type (<S>)
// Example: <S>This is a string</S>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/052b8c32-735b-49c0-8c24-bb32a5c871ce

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliString {
    pub value: String,
    pub name: Option<String>,
}

impl CliString {
    pub fn new(name: Option<&str>, value: &str) -> CliString {
        CliString {
            name: name.map(|s| s.to_string()),
            value: value.to_string(),
        }
    }
}

// Character type (<C>)
// Example: <C>97</C>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/ff6f9767-a0a5-4cca-b091-4f15afc6e6d8

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliChar {
    pub value: char,
    pub name: Option<String>,
}

impl CliChar {
    pub fn new(name: Option<&str>, value: char) -> CliChar {
        CliChar {
            name: name.map(|s| s.to_string()),
            value: value,
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliChar> {
        let value = value.parse::<u16>().ok()?;
        let mut chars = std::char::decode_utf16(vec![value]).collect::<Vec<_>>();
        let value: char = chars.pop().unwrap().ok()?;
        Some(Self::new(name, value))
    }
}

// Boolean type (<B>)
// Example: <B>true</B>
// https://www.w3.org/TR/xmlschema-2/#boolean
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/8b4b1067-4b58-46d5-b1c9-b881b6e7a0aa

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliBool {
    pub value: bool,
    pub name: Option<String>,
}

impl CliBool {
    pub fn new(name: Option<&str>, value: bool) -> CliBool {
        CliBool {
            name: name.map(|s| s.to_string()),
            value: value,
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliBool> {
        let value = value.parse::<bool>().ok()?;
        Some(Self::new(name, value))
    }
}

// Date/Time type (<DT>)
// Example: <DT>2008-04-11T10:42:32.2731993-07:00</DT>
// https://www.w3.org/TR/xmlschema-2/#dateTime
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/a3b75b8d-ad7e-4649-bb82-cfa70f54fb8c

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliDateTime {
    pub value: DateTime,
    pub name: Option<String>,
}

impl CliDateTime {
    pub fn new(name: Option<&str>, value: DateTime) -> CliDateTime {
        CliDateTime {
            name: name.map(|s| s.to_string()),
            value: value,
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliDateTime> {
        let value = DateTime::parse(value)?;
        Some(Self::new(name, value))
    }
}

// Duration type (<TS>)
// Example: <TS>PT9.0269026S</TS>
// https://www.w3.org/TR/xmlschema-2/#duration
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/434cd15d-8fb3-462c-a004-bcd0d3a60201

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliDuration {
    pub value: Duration,
    pub name: Option<String>,
}

impl CliDuration {
    pub fn new(name: Option<&str>, value: Duration) -> CliDuration {
        CliDuration {
            name: name.map(|s| s.to_string()),
            value: value,
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliDuration> {
        let value = parse_iso8601_duration(value)?;
        Some(Self::new(name, value))
    }
}

// Unsigned Byte type (<By>)
// Example: <By>254</By>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/6e25153d-77b6-4e21-b5fa-6f986895171a

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliUInt8 {
    pub value: u8,
    pub name: Option<String>,
}

impl CliUInt8 {
    pub fn new(name: Option<&str>, value: u8) -> CliUInt8 {
        CliUInt8 {
            name: name.map(|s| s.to_string()),
            value: value,
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliUInt8> {
        let value = value.parse::<u8>().ok()?;
        Some(Self::new(name, value))
    }
}

// Signed Byte type (<SB>)
// Example: <SB>-127</SB>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/8046c418-1531-4c43-9b9d-fb9bceace0db

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliInt8 {
    pub value: i8,
    pub name: Option<String>,
}

impl CliInt8 {
    pub fn new(name: Option<&str>, value: i8) -> CliInt8 {
        CliInt8 {
            name: name.map(|s| s.to_string()),
            value: value,
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliInt8> {
        let value = value.parse::<i8>().ok()?;
        Some(Self::new(name, value))
    }
}

// Unsigned Short type (<U16>)
// Example: <U16>65535</U16>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/33751ca7-90d0-4b5e-a04f-2d8798cfb419

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliUInt16 {
    pub value: u16,
    pub name: Option<String>,
}

impl CliUInt16 {
    pub fn new(name: Option<&str>, value: u16) -> CliUInt16 {
        CliUInt16 {
            name: name.map(|s| s.to_string()),
            value: value,
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliUInt16> {
        let value = value.parse::<u16>().ok()?;
        Some(Self::new(name, value))
    }
}

// Signed Short type (<I16>)
// Example: <I16>-16767</I16>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/e0ed596d-0aea-40bb-a254-285b71188214

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliInt16 {
    pub value: i16,
    pub name: Option<String>,
}

impl CliInt16 {
    pub fn new(name: Option<&str>, value: i16) -> CliInt16 {
        CliInt16 {
            name: name.map(|s| s.to_string()),
            value: value,
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliInt16> {
        let value = value.parse::<i16>().ok()?;
        Some(Self::new(name, value))
    }
}

// Unsigned Int type (<U32>)
// Example: <U32>4294967295</U32>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/7b904471-3519-4a6a-900b-8053ad975c08

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliUInt32 {
    pub value: u32,
    pub name: Option<String>,
}

impl CliUInt32 {
    pub fn new(name: Option<&str>, value: u32) -> CliUInt32 {
        CliUInt32 {
            name: name.map(|s| s.to_string()),
            value: value,
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliUInt32> {
        let value = value.parse::<u32>().ok()?;
        Some(Self::new(name, value))
    }
}

// Signed Int type (<I32>)
// Example: <I32>-2147483648</I32>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/9eef96ba-1876-427b-9450-75a1b28f5668

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliInt32 {
    pub value: i32,
    pub name: Option<String>,
}

impl CliInt32 {
    pub fn new(name: Option<&str>, value: i32) -> CliInt32 {
        CliInt32 {
            name: name.map(|s| s.to_string()),
            value: value,
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliInt32> {
        let value = value.parse::<i32>().ok()?;
        Some(Self::new(name, value))
    }
}

// Unsigned Long type (<U64>)
// Example: <U64>18446744073709551615</U64>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/d92cd5d2-59c6-4a61-b517-9fc48823cb4d

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliUInt64 {
    pub value: u64,
    pub name: Option<String>,
}

impl CliUInt64 {
    pub fn new(name: Option<&str>, value: u64) -> CliUInt64 {
        CliUInt64 {
            name: name.map(|s| s.to_string()),
            value: value,
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliUInt64> {
        let value = value.parse::<u64>().ok()?;
        Some(Self::new(name, value))
    }
}

// Signed Long type (<I64>)
// Example: <I64>-9223372036854775808</I64>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/de124e86-3f8c-426a-ab75-47fdb4597c62

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliInt64 {
    pub value: i64,
    pub name: Option<String>,
}

impl CliInt64 {
    pub fn new(name: Option<&str>, value: i64) -> CliInt64 {
        CliInt64 {
            name: name.map(|s| s.to_string()),
            value: value,
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliInt64> {
        let value = value.parse::<i64>().ok()?;
        Some(Self::new(name, value))
    }
}

// Float type (<Sg>)
// Example: <Sg>12.34</Sg>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/d8a5a9ab-5f52-4175-96a3-c29afb7b82b8

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CliFloat {
    pub value: f32,
    pub name: Option<String>,
}

impl CliFloat {
    pub fn new(name: Option<&str>, value: f32) -> CliFloat {
        CliFloat {
            name: name.map(|s| s.to_string()),
            value: value,
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliFloat> {
        let value = value.parse::<f32>().ok()?;
        Some(Self::new(name, value))
    }
}

// Double type (<Db>)
// Example: <Db>12.34</Db>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/02fa08c5-139c-4e98-a13e-45784b4eabde

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CliDouble {
    pub value: f64,
    pub name: Option<String>,
}

impl CliDouble {
    pub fn new(name: Option<&str>, value: f64) -> CliDouble {
        CliDouble {
            name: name.map(|s| s.to_string()),
            value: value,
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliDouble> {
        let value = value.parse::<f64>().ok()?;
        Some(Self::new(name, value))
    }
}

// Array of Bytes type (<AB>)
// Example: <BA>AQIDBA==</BA>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/489ed886-34d2-4306-a2f5-73843c219b14

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliBuffer {
    pub value: Vec<u8>,
    pub name: Option<String>,
}

impl CliBuffer {
    pub fn new(name: Option<&str>, value: Vec<u8>) -> CliBuffer {
        CliBuffer {
            name: name.map(|s| s.to_string()),
            value: value,
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliBuffer> {
        let value = base64::decode(value).ok()?;
        Some(Self::new(name, value))
    }
}

// GUID type (<G>)
// Example: <G>792e5b37-4505-47ef-b7d2-8711bb7affa8</G>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/c30c37fa-692d-49c7-bb86-b3179a97e106

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliGuid {
    pub value: Uuid,
    pub name: Option<String>,
}

impl CliGuid {
    pub fn new(name: Option<&str>, value: Uuid) -> CliGuid {
        CliGuid {
            name: name.map(|s| s.to_string()),
            value: value,
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliGuid> {
        let value = Uuid::parse_str(value).ok()?;
        Some(Self::new(name, value))
    }
}

// URI type (<URI>)
// Example: <URI>http://www.microsoft.com/</URI>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/4ac73ac2-5cf7-4669-b4de-c8ba19a13186

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliUri {
    pub value: Url,
    pub name: Option<String>,
}

impl CliUri {
    pub fn new(name: Option<&str>, value: Url) -> CliUri {
        CliUri {
            name: name.map(|s| s.to_string()),
            value: value,
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliUri> {
        let value = Url::parse(value).ok()?;
        Some(Self::new(name, value))
    }
}

impl Default for CliUri {
    fn default() -> Self {
        CliUri::new(None, Url::parse("http://default").unwrap())
    }
}

// Version type (<Version>)
// Example: <Version>6.2.1.3</Version>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/390db910-e035-4f97-80fd-181a008ff6f8

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliVersion {
    pub value: String,
    pub name: Option<String>,
}

impl CliVersion {
    pub fn new(name: Option<&str>, value: &str) -> CliVersion {
        CliVersion {
            name: name.map(|s| s.to_string()),
            value: value.to_string(),
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliVersion> {
        Some(Self::new(name, value))
    }
}

// XML Document type (<XD>)
// Example: <XD>&lt;name attribute="value"&gt;Content&lt;/name&gt;</XD>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/df5908ab-bb4d-45e4-8adc-7258e5a9f537

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliXmlDocument {
    pub value: String,
    pub name: Option<String>,
}

impl CliXmlDocument {
    pub fn new(name: Option<&str>, value: &str) -> CliXmlDocument {
        CliXmlDocument {
            name: name.map(|s| s.to_string()),
            value: value.to_string(),
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliXmlDocument> {
        Some(Self::new(name, value))
    }
}

// Script Block type (<SBK>)
// Example: <SBK>get-command -type cmdlet</SBK>
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/306af1be-6be5-4074-acc9-e29bd32f3206

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliScriptBlock {
    pub value: String,
    pub name: Option<String>,
}

impl CliScriptBlock {
    pub fn new(name: Option<&str>, value: &str) -> CliScriptBlock {
        CliScriptBlock {
            name: name.map(|s| s.to_string()),
            value: value.to_string(),
        }
    }

    pub fn new_from_str(name: Option<&str>, value: &str) -> Option<CliScriptBlock> {
        Some(Self::new(name, value))
    }
}

// Generic CLI XML Value type

#[derive(Debug, Clone)]
pub enum CliValue {
    Null,
    CliObject(CliObject),
    CliString(CliString),
    CliChar(CliChar),
    CliBool(CliBool),
    CliDateTime(CliDateTime),
    CliDuration(CliDuration),
    CliUInt8(CliUInt8),
    CliInt8(CliInt8),
    CliUInt16(CliUInt16),
    CliInt16(CliInt16),
    CliUInt32(CliUInt32),
    CliInt32(CliInt32),
    CliUInt64(CliUInt64),
    CliInt64(CliInt64),
    CliFloat(CliFloat),
    CliDouble(CliDouble),
    CliBuffer(CliBuffer),
    CliGuid(CliGuid),
    CliUri(CliUri),
    CliVersion(CliVersion),
    CliXmlDocument(CliXmlDocument),
    CliScriptBlock(CliScriptBlock),
}

impl CliValue {
    pub fn get_name(&self) -> Option<&str> {
        match &*self {
            CliValue::CliObject(prop) => prop.name.as_deref(),
            CliValue::CliString(prop) => prop.name.as_deref(),
            CliValue::CliChar(prop) => prop.name.as_deref(),
            CliValue::CliBool(prop) => prop.name.as_deref(),
            CliValue::CliDateTime(prop) => prop.name.as_deref(),
            CliValue::CliDuration(prop) => prop.name.as_deref(),
            CliValue::CliUInt8(prop) => prop.name.as_deref(),
            CliValue::CliInt8(prop) => prop.name.as_deref(),
            CliValue::CliUInt16(prop) => prop.name.as_deref(),
            CliValue::CliInt16(prop) => prop.name.as_deref(),
            CliValue::CliUInt32(prop) => prop.name.as_deref(),
            CliValue::CliInt32(prop) => prop.name.as_deref(),
            CliValue::CliUInt64(prop) => prop.name.as_deref(),
            CliValue::CliInt64(prop) => prop.name.as_deref(),
            CliValue::CliFloat(prop) => prop.name.as_deref(),
            CliValue::CliDouble(prop) => prop.name.as_deref(),
            CliValue::CliBuffer(prop) => prop.name.as_deref(),
            CliValue::CliGuid(prop) => prop.name.as_deref(),
            CliValue::CliUri(prop) => prop.name.as_deref(),
            CliValue::CliVersion(prop) => prop.name.as_deref(),
            CliValue::CliXmlDocument(prop) => prop.name.as_deref(),
            CliValue::CliScriptBlock(prop) => prop.name.as_deref(),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        match *self {
            CliValue::Null => true,
            _ => false,
        }
    }

    pub fn is_object(&self) -> bool {
        match *self {
            CliValue::CliObject(_) => true,
            _ => false,
        }
    }

    pub fn as_object(&self) -> Option<&CliObject> {
        match &*self {
            CliValue::CliObject(prop) => Some(&prop),
            _ => None,
        }
    }

    pub fn is_string(&self) -> bool {
        match *self {
            CliValue::CliString(_) => true,
            _ => false,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match &*self {
            CliValue::CliString(prop) => Some(&prop.value),
            _ => None,
        }
    }

    pub fn is_bool(&self) -> bool {
        match *self {
            CliValue::CliBool(_) => true,
            _ => false,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match &*self {
            CliValue::CliBool(prop) => Some(prop.value),
            _ => None,
        }
    }

    pub fn is_datetime(&self) -> bool {
        match *self {
            CliValue::CliDateTime(_) => true,
            _ => false,
        }
    }

    pub fn as_datetime(&self) -> Option<&DateTime> {
        match &*self {
            CliValue::CliDateTime(prop) => Some(&prop.value),
            _ => None,
        }
    }

    pub fn is_duration(&self) -> bool {
        match *self {
            CliValue::CliDuration(_) => true,
            _ => false,
        }
    }

    pub fn as_duration(&self) -> Option<&Duration> {
        match &*self {
            CliValue::CliDuration(prop) => Some(&prop.value),
            _ => None,
        }
    }

    pub fn is_uint8(&self) -> bool {
        match *self {
            CliValue::CliUInt8(_) => true,
            _ => false,
        }
    }

    pub fn as_uint8(&self) -> Option<u8> {
        match &*self {
            CliValue::CliUInt8(prop) => Some(prop.value),
            _ => None,
        }
    }

    pub fn is_int8(&self) -> bool {
        match *self {
            CliValue::CliInt8(_) => true,
            _ => false,
        }
    }

    pub fn as_int8(&self) -> Option<i8> {
        match &*self {
            CliValue::CliInt8(prop) => Some(prop.value),
            _ => None,
        }
    }

    pub fn is_uint16(&self) -> bool {
        match *self {
            CliValue::CliUInt16(_) => true,
            _ => false,
        }
    }

    pub fn as_uint16(&self) -> Option<u16> {
        match &*self {
            CliValue::CliUInt8(prop) => Some(prop.value as u16),
            CliValue::CliUInt16(prop) => Some(prop.value),
            _ => None,
        }
    }

    pub fn is_int16(&self) -> bool {
        match *self {
            CliValue::CliInt16(_) => true,
            _ => false,
        }
    }

    pub fn as_int16(&self) -> Option<i16> {
        match &*self {
            CliValue::CliInt8(prop) => Some(prop.value as i16),
            CliValue::CliInt16(prop) => Some(prop.value),
            _ => None,
        }
    }

    pub fn is_uint32(&self) -> bool {
        match *self {
            CliValue::CliUInt32(_) => true,
            _ => false,
        }
    }

    pub fn as_uint32(&self) -> Option<u32> {
        match &*self {
            CliValue::CliUInt8(prop) => Some(prop.value as u32),
            CliValue::CliUInt16(prop) => Some(prop.value as u32),
            CliValue::CliUInt32(prop) => Some(prop.value),
            _ => None,
        }
    }

    pub fn is_int32(&self) -> bool {
        match *self {
            CliValue::CliInt32(_) => true,
            _ => false,
        }
    }

    pub fn as_int32(&self) -> Option<i32> {
        match &*self {
            CliValue::CliInt8(prop) => Some(prop.value as i32),
            CliValue::CliInt16(prop) => Some(prop.value as i32),
            CliValue::CliInt32(prop) => Some(prop.value),
            _ => None,
        }
    }

    pub fn is_uint64(&self) -> bool {
        match *self {
            CliValue::CliUInt64(_) => true,
            _ => false,
        }
    }

    pub fn as_uint64(&self) -> Option<u64> {
        match &*self {
            CliValue::CliUInt8(prop) => Some(prop.value as u64),
            CliValue::CliUInt16(prop) => Some(prop.value as u64),
            CliValue::CliUInt32(prop) => Some(prop.value as u64),
            CliValue::CliUInt64(prop) => Some(prop.value),
            _ => None,
        }
    }

    pub fn is_int64(&self) -> bool {
        match *self {
            CliValue::CliInt64(_) => true,
            _ => false,
        }
    }

    pub fn as_int64(&self) -> Option<i64> {
        match &*self {
            CliValue::CliInt8(prop) => Some(prop.value as i64),
            CliValue::CliInt16(prop) => Some(prop.value as i64),
            CliValue::CliInt32(prop) => Some(prop.value as i64),
            CliValue::CliInt64(prop) => Some(prop.value),
            _ => None,
        }
    }

    pub fn is_float(&self) -> bool {
        match *self {
            CliValue::CliFloat(_) => true,
            _ => false,
        }
    }

    pub fn as_float(&self) -> Option<f32> {
        match &*self {
            CliValue::CliFloat(prop) => Some(prop.value),
            _ => None,
        }
    }

    pub fn is_double(&self) -> bool {
        match *self {
            CliValue::CliDouble(_) => true,
            _ => false,
        }
    }

    pub fn as_double(&self) -> Option<f64> {
        match &*self {
            CliValue::CliFloat(prop) => Some(prop.value as f64),
            CliValue::CliDouble(prop) => Some(prop.value),
            _ => None,
        }
    }

    pub fn is_buffer(&self) -> bool {
        match *self {
            CliValue::CliBuffer(_) => true,
            _ => false,
        }
    }

    pub fn as_bytes(&self) -> Option<&Vec<u8>> {
        match &*self {
            CliValue::CliBuffer(prop) => Some(&prop.value),
            _ => None,
        }
    }

    pub fn is_guid(&self) -> bool {
        match *self {
            CliValue::CliGuid(_) => true,
            _ => false,
        }
    }

    pub fn as_guid(&self) -> Option<&Uuid> {
        match &*self {
            CliValue::CliGuid(prop) => Some(&prop.value),
            _ => None,
        }
    }

    pub fn is_uri(&self) -> bool {
        match *self {
            CliValue::CliUri(_) => true,
            _ => false,
        }
    }

    pub fn as_uri(&self) -> Option<&Url> {
        match &*self {
            CliValue::CliUri(prop) => Some(&prop.value),
            _ => None,
        }
    }

    pub fn is_version(&self) -> bool {
        match *self {
            CliValue::CliVersion(_) => true,
            _ => false,
        }
    }

    pub fn as_version(&self) -> Option<&str> {
        match &*self {
            CliValue::CliVersion(prop) => Some(&prop.value),
            _ => None,
        }
    }

    pub fn is_xml_document(&self) -> bool {
        match *self {
            CliValue::CliXmlDocument(_) => true,
            _ => false,
        }
    }

    pub fn as_xml_document(&self) -> Option<&str> {
        match &*self {
            CliValue::CliXmlDocument(prop) => Some(&prop.value),
            _ => None,
        }
    }

    pub fn is_script_block(&self) -> bool {
        match *self {
            CliValue::CliScriptBlock(_) => true,
            _ => false,
        }
    }

    pub fn as_script_block(&self) -> Option<&str> {
        match &*self {
            CliValue::CliScriptBlock(prop) => Some(&prop.value),
            _ => None,
        }
    }
}

fn try_get_ref_id_attr<B>(reader: &Reader<B>, event: &events::BytesStart) -> Option<String> {
    let attr = event.try_get_attribute("RefId").ok().unwrap()?;
    let value = attr.decode_and_unescape_value(&reader).ok()?;
    Some(value.to_string())
}

fn try_get_name_attr<B>(reader: &Reader<B>, event: &events::BytesStart) -> Option<String> {
    let attr = event.try_get_attribute("N").ok().unwrap()?;
    let value = attr.decode_and_unescape_value(&reader).ok()?;
    Some(value.to_string())
}

pub fn parse_cli_xml(cli_xml: &str) -> Vec<CliObject> {
    let mut reader = Reader::from_str(cli_xml);
    reader.trim_text(true);

    let mut objs: Vec<CliObject> = Vec::new();
    let mut obj = CliObject::default();

    loop {
        let event = reader.read_event();
        match event {
            Ok(Event::Start(event)) => {
                match event.name().as_ref() {
                    b"Objs" => {}
                    b"Obj" => {
                        if let Some(ref_id) = try_get_ref_id_attr(&reader, &event) {
                            obj.ref_id = Some(ref_id);
                        }
                    }
                    b"TN" => {
                        if let Some(_ref_id) = try_get_ref_id_attr(&reader, &event) {
                            //println!("TN RefId={}", ref_id);
                        }
                    }
                    b"T" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        obj.type_names.push(txt.to_string());
                    }
                    b"TNRef" => {
                        if let Some(_ref_id) = try_get_ref_id_attr(&reader, &event) {
                            //println!("TNRef RefId={}", ref_id);
                        }
                    }
                    b"ToString" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        obj.string_repr = Some(txt.to_string());
                    }
                    b"Props" => {
                        // Adapted Properties
                        // https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/173c30d7-b0a6-4aad-9b00-9891c441b0f3
                    }
                    b"MS" => {
                        // Extended Properties
                        // https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-psrp/4cca6d92-4a8e-4406-91cb-0235a98f7d6f
                    }
                    b"LST" => {}
                    b"IE" => {}
                    b"B" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliBool::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliBool(val));
                    }
                    b"S" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliString::new(prop_name.as_deref(), &txt);
                        obj.values.push(CliValue::CliString(val));
                    }
                    b"C" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliChar::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliChar(val));
                    }
                    b"By" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliUInt8::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliUInt8(val));
                    }
                    b"SB" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliInt8::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliInt8(val));
                    }
                    b"U16" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliUInt16::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliUInt16(val));
                    }
                    b"I16" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliInt16::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliInt16(val));
                    }
                    b"U32" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliUInt32::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliUInt32(val));
                    }
                    b"I32" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliInt32::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliInt32(val));
                    }
                    b"U64" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliUInt64::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliUInt64(val));
                    }
                    b"I64" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliInt64::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliInt64(val));
                    }
                    b"DT" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliDateTime::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliDateTime(val));
                    }
                    b"TS" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliDuration::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliDuration(val));
                    }
                    b"Sg" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliFloat::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliFloat(val));
                    }
                    b"Db" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliDouble::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliDouble(val));
                    }
                    b"AB" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliBuffer::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliBuffer(val));
                    }
                    b"G" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliGuid::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliGuid(val));
                    }
                    b"URI" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliUri::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliUri(val));
                    }
                    b"Version" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliVersion::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliVersion(val));
                    }
                    b"XD" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliXmlDocument::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliXmlDocument(val));
                    }
                    b"SBK" => {
                        let txt = reader.read_text(event.name()).unwrap();
                        let prop_name = try_get_name_attr(&reader, &event);
                        let val = CliScriptBlock::new_from_str(prop_name.as_deref(), &txt).unwrap();
                        obj.values.push(CliValue::CliScriptBlock(val));
                    }
                    b"Nil" => {
                        obj.values.push(CliValue::Null);
                    }
                    _ => {
                        let event_name = event.name();
                        let tag_name = String::from_utf8_lossy(event_name.as_ref());
                        println!("unsupported: {}", &tag_name);
                    }
                }
            }
            Ok(Event::End(event)) => match event.name().as_ref() {
                b"Obj" => {
                    objs.push(obj);
                    obj = CliObject::default();
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(event) => panic!(
                "Error at position {}: {:?}",
                reader.buffer_position(),
                event
            ),
            _ => (),
        }
    }

    objs
}
