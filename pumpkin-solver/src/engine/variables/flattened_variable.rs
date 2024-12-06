use std::fmt::Debug;

use crate::variables::DomainId;

#[derive(Debug, Clone, Copy)]
pub struct FlattenedVariable {
    pub id: DomainId,
    pub scale: i32,
    pub offset: i32,
}

impl FlattenedVariable {
    pub fn new(id: DomainId, scale: i32, offset: i32) -> Self {
        Self { id, scale, offset }
    }
}
