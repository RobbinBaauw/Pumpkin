use log::warn;

use super::variable_selector::find_extremum;
use super::variable_selector::Direction;
use crate::basic_types::DomainId;
use crate::branching::SelectionContext;
use crate::branching::VariableSelector;
use crate::pumpkin_assert_simple;

/// A [`VariableSelector`] which selects the variable with the largest difference between the two
/// smallest values in its domain.
///
/// Currently, due to the implementation of the domains, in the worst-case this selector will go
/// through all variables and all values between the upper-bound and lower-bound.
#[derive(Debug)]
pub struct MaxRegret<Var> {
    variables: Vec<Var>,
}

impl<Var: Copy> MaxRegret<Var> {
    pub fn new(variables: &[Var]) -> Self {
        if variables.is_empty() {
            warn!("The MaxRegret variable selector was not provided with any variables");
        }
        MaxRegret {
            variables: variables.to_vec(),
        }
    }
}

impl VariableSelector<DomainId> for MaxRegret<DomainId> {
    fn select_variable(&mut self, context: &SelectionContext) -> Option<DomainId> {
        find_extremum(
            &self.variables,
            |variable| {
                let smallest_value = context.lower_bound(variable);
                let second_smallest_value = (smallest_value + 1..=context.upper_bound(variable))
                    .find(|bound| context.contains(variable, *bound))
                    .expect("Expected at least 2 values in the domain");
                pumpkin_assert_simple!(second_smallest_value > smallest_value);
                second_smallest_value - smallest_value
            },
            context,
            Direction::Maximum,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::basic_types::tests::TestRandom;
    use crate::branching::MaxRegret;
    use crate::branching::SelectionContext;
    use crate::branching::VariableSelector;

    #[test]
    fn test_correctly_selected() {
        let (mut assignments_integer, assignments_propositional, mediator) =
            SelectionContext::create_for_testing(2, 0, Some(vec![(0, 10), (5, 20)]));
        let mut test_rng = TestRandom::default();
        let integer_variables = assignments_integer.get_domains().collect::<Vec<_>>();
        let mut strategy = MaxRegret::new(&integer_variables);

        let _ = assignments_integer.remove_value_from_domain(integer_variables[1], 6, None);

        {
            let context = SelectionContext::new(
                &assignments_integer,
                &assignments_propositional,
                &mediator,
                &mut test_rng,
            );

            let selected = strategy.select_variable(&context);
            assert!(selected.is_some());
            assert_eq!(selected.unwrap(), integer_variables[1]);
        }

        let _ = assignments_integer.remove_value_from_domain(integer_variables[0], 1, None);
        let _ = assignments_integer.remove_value_from_domain(integer_variables[0], 2, None);

        let context = SelectionContext::new(
            &assignments_integer,
            &assignments_propositional,
            &mediator,
            &mut test_rng,
        );

        let selected = strategy.select_variable(&context);
        assert!(selected.is_some());
        assert_eq!(selected.unwrap(), integer_variables[0])
    }

    #[test]
    fn fixed_variables_are_not_selected() {
        let (assignments_integer, assignments_propositional, mediator) =
            SelectionContext::create_for_testing(2, 0, Some(vec![(10, 10), (20, 20)]));
        let mut test_rng = TestRandom::default();
        let context = SelectionContext::new(
            &assignments_integer,
            &assignments_propositional,
            &mediator,
            &mut test_rng,
        );
        let integer_variables = context.get_domains().collect::<Vec<_>>();

        let mut strategy = MaxRegret::new(&integer_variables);
        let selected = strategy.select_variable(&context);
        assert!(selected.is_none());
    }
}