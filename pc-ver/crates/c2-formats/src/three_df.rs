//! `.3DF` static models (sun, moon, compass, binoculars).
//! Loaded by `Resources.cpp: LoadModelEx`.

use crate::error::FormatError;
use crate::model::StaticModel;
use crate::reader::Reader;
use alloc::vec::Vec;

pub fn parse(data: &[u8]) -> Result<StaticModel, FormatError> {
    let mut r = Reader::new(data);
    let m = StaticModel::read(&mut r)?;
    r.finish("3df")?;
    Ok(m)
}

pub fn write(model: &StaticModel) -> Vec<u8> {
    let mut w = Vec::new();
    model.write(&mut w);
    w
}
