use std::fmt;

use serde_json::{Map, Value};

use crate::model::AwAvatar;

pub const EXTENSION: &str = "AW_avatar";

const MAGIC: &[u8; 4] = b"glTF";
const VERSION: u32 = 2;
const CHUNK_JSON: &[u8; 4] = b"JSON";
const CHUNK_BIN: &[u8; 4] = b"BIN\0";

/// An avatar file: the glTF document without `AW_avatar`, its binary chunk, and the model.
/// The writer pads `bin` to four bytes, so it reads back padded.
#[derive(Clone, Debug, PartialEq)]
pub struct Avatar {
    pub gltf: Map<String, Value>,
    pub bin: Vec<u8>,
    pub model: AwAvatar,
}

#[derive(Debug)]
pub enum Error {
    Header(&'static str),
    Json(serde_json::Error),
    Model(serde_json::Error),
    NotAnObject,
    MissingExtension,
    Mismatch(&'static str),
    Unwritable(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Header(what) => write!(f, "not a glTF binary: {what}"),
            Error::Json(e) => write!(f, "glTF JSON chunk: {e}"),
            Error::Model(e) => write!(f, "{EXTENSION}: {e}"),
            Error::NotAnObject => write!(f, "glTF JSON chunk is not an object"),
            Error::MissingExtension => write!(f, "no {EXTENSION} extension on the glTF root"),
            Error::Mismatch(what) => write!(
                f,
                "{EXTENSION}.{what} does not have one entry per glTF {what}"
            ),
            Error::Unwritable(what) => write!(f, "cannot write: {what}"),
        }
    }
}

impl std::error::Error for Error {}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, Error> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .ok_or(Error::Header("truncated"))
}

/// The chunk at `at`: its type, its data and where the next chunk starts.
fn chunk(bytes: &[u8], at: usize) -> Result<(&[u8], &[u8], usize), Error> {
    let len = u32_at(bytes, at)? as usize;
    let kind = bytes
        .get(at + 4..at + 8)
        .ok_or(Error::Header("truncated"))?;
    let end = (at + 8)
        .checked_add(len)
        .filter(|end| *end <= bytes.len())
        .ok_or(Error::Header("chunk overruns the file"))?;
    Ok((kind, &bytes[at + 8..end], end))
}

pub fn read(bytes: &[u8]) -> Result<Avatar, Error> {
    if bytes.get(0..4) != Some(MAGIC) {
        return Err(Error::Header("bad magic"));
    }
    if u32_at(bytes, 4)? != VERSION {
        return Err(Error::Header("not version 2"));
    }
    if u32_at(bytes, 8)? as usize != bytes.len() {
        return Err(Error::Header("length does not match the file"));
    }
    let (kind, json, mut next) = chunk(bytes, 12)?;
    if kind != CHUNK_JSON {
        return Err(Error::Header("the first chunk is not JSON"));
    }
    let mut bin = Vec::new();
    let mut second = true;
    while next < bytes.len() {
        let (kind, data, end) = chunk(bytes, next)?;
        if kind == CHUNK_BIN {
            if !second {
                return Err(Error::Header("a binary chunk that is not the second chunk"));
            }
            bin = data.to_vec();
        }
        second = false;
        next = end;
    }
    let Value::Object(mut gltf) = serde_json::from_slice(json).map_err(Error::Json)? else {
        return Err(Error::NotAnObject);
    };
    let model = take_extension(&mut gltf).ok_or(Error::MissingExtension)?;
    let model = serde_json::from_value(model).map_err(Error::Model)?;
    check_counts(&gltf, &model)?;
    Ok(Avatar { gltf, bin, model })
}

fn check_counts(gltf: &Map<String, Value>, model: &AwAvatar) -> Result<(), Error> {
    let count = |key: &str| gltf.get(key).and_then(Value::as_array).map_or(0, Vec::len);
    if model.nodes.len() != count("nodes") {
        return Err(Error::Mismatch("nodes"));
    }
    if model.materials.len() != count("materials") {
        return Err(Error::Mismatch("materials"));
    }
    Ok(())
}

fn take_extension(gltf: &mut Map<String, Value>) -> Option<Value> {
    let extensions = gltf.get_mut("extensions")?.as_object_mut()?;
    let model = extensions.shift_remove(EXTENSION)?;
    if extensions.is_empty() {
        gltf.shift_remove("extensions");
    }
    if let Some(Value::Array(used)) = gltf.get_mut("extensionsUsed") {
        used.retain(|name| name != EXTENSION);
        if used.is_empty() {
            gltf.shift_remove("extensionsUsed");
        }
    }
    Some(model)
}

pub fn write(avatar: &Avatar) -> Result<Vec<u8>, Error> {
    check_counts(&avatar.gltf, &avatar.model)?;
    let model = serde_json::to_value(&avatar.model).map_err(Error::Model)?;
    if serde_json::from_value::<AwAvatar>(model.clone())
        .ok()
        .as_ref()
        != Some(&avatar.model)
    {
        return Err(Error::Unwritable(
            "the model holds a value JSON cannot carry, such as NaN",
        ));
    }
    let mut gltf = avatar.gltf.clone();
    let Value::Array(used) = gltf
        .entry("extensionsUsed")
        .or_insert_with(|| Value::Array(Vec::new()))
    else {
        return Err(Error::Unwritable("extensionsUsed is not an array"));
    };
    if !used.iter().any(|name| name == EXTENSION) {
        used.push(Value::from(EXTENSION));
    }
    let Value::Object(extensions) = gltf
        .entry("extensions")
        .or_insert_with(|| Value::Object(Map::new()))
    else {
        return Err(Error::Unwritable("extensions is not an object"));
    };
    extensions.insert(EXTENSION.to_owned(), model);
    let mut json = serde_json::to_vec(&gltf).map_err(Error::Json)?;
    json.resize(json.len().next_multiple_of(4), b' ');
    let mut bin = avatar.bin.clone();
    bin.resize(bin.len().next_multiple_of(4), 0);

    let bin_chunk = if bin.is_empty() { 0 } else { 8 + bin.len() };
    let too_large = Error::Unwritable("larger than a glTF binary can hold");
    let total = u32::try_from(12 + 8 + json.len() + bin_chunk).map_err(|_| too_large)?;
    let mut out = Vec::with_capacity(total as usize);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(CHUNK_JSON);
    out.extend_from_slice(&json);
    if !bin.is_empty() {
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(CHUNK_BIN);
        out.extend_from_slice(&bin);
    }
    Ok(out)
}
