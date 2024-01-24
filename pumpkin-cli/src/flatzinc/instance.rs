use std::fmt::Display;
use std::fmt::Write;
use std::rc::Rc;

use pumpkin_lib::basic_types::DomainId;
use pumpkin_lib::basic_types::Literal;

#[derive(Default)]
pub struct FlatZincInstance {
    pub(super) outputs: Vec<Output>,
}

impl FlatZincInstance {
    pub fn outputs(&self) -> impl Iterator<Item = &Output> + '_ {
        self.outputs.iter()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    Bool(VariableOutput<Literal>),
    Int(VariableOutput<DomainId>),
    ArrayOfBool(ArrayOutput<Literal>),
    ArrayOfInt(ArrayOutput<DomainId>),
}

impl Output {
    pub fn bool(id: Rc<str>, literal: Literal) -> Output {
        Output::Bool(VariableOutput {
            id,
            variable: literal,
        })
    }

    pub fn array_of_bool(id: Rc<str>, shape: Box<[(i32, i32)]>, contents: Rc<[Literal]>) -> Output {
        Output::ArrayOfBool(ArrayOutput {
            id,
            shape,
            contents,
        })
    }

    pub fn int(id: Rc<str>, domain_id: DomainId) -> Output {
        Output::Int(VariableOutput {
            id,
            variable: domain_id,
        })
    }

    pub fn array_of_int(id: Rc<str>, shape: Box<[(i32, i32)]>, contents: Rc<[DomainId]>) -> Output {
        Output::ArrayOfInt(ArrayOutput {
            id,
            shape,
            contents,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariableOutput<T> {
    id: Rc<str>,
    variable: T,
}

impl<T> VariableOutput<T> {
    pub fn print_value<V: Display>(&self, value: impl FnOnce(&T) -> V) {
        println!("{} = {};", self.id, value(&self.variable));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArrayOutput<T> {
    id: Rc<str>,
    /// The shape of the array is a sequence of index sets. The number of elements in this sequence
    /// corresponds to the dimensionality of the array, and the element in the sequence at index i denotes
    /// the index set used in dimension i.
    /// Example: [(1, 5), (2, 4)] describes a 2d array, where the first dimension in indexed with
    /// an element of 1..5, and the second dimension is indexed with an element from 2..4.
    shape: Box<[(i32, i32)]>,
    contents: Rc<[T]>,
}

impl<T> ArrayOutput<T> {
    pub fn print_value<V: Display>(&self, value: impl Fn(&T) -> V) {
        let mut array_buf = String::new();

        for element in self.contents.iter() {
            let value = value(element);
            write!(array_buf, "{value}, ").unwrap();
        }

        let mut shape_buf = String::new();
        for (min, max) in self.shape.iter() {
            write!(shape_buf, "{min}..{max}, ").unwrap();
        }

        if !array_buf.is_empty() {
            // Remove trailing comma and space.
            array_buf.truncate(array_buf.len() - 2);
        }

        let num_dimensions = self.shape.len();
        println!(
            "{} = array{num_dimensions}d({shape_buf}[{array_buf}]);",
            self.id
        );
    }
}
