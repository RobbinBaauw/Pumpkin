use std::fmt::Debug;

#[derive(Debug, Clone, Copy)]
pub struct FlattenedVariable {
    pub id: u32,
    pub scale: i32,
    pub offset: i32,
}

impl FlattenedVariable {
    pub fn new(id: u32, scale: i32, offset: i32) -> Self {
        Self { id, scale, offset }
    }
}