use super::*;

/// Operator evaluator for comparing attribute values
#[derive(Clone)]
pub struct OperatorEvaluator;

impl OperatorEvaluator {
    pub fn new() -> Self {
        Self
    }

    /// Evaluate an operator comparison
    pub fn evaluate_operator(
        &self,
        operator: Operator,
        left: &AttributeValue,
        right: &AttributeValue,
    ) -> Result<bool, AppError> {
        match operator {
            Operator::Equal => Ok(self.values_equal(left, right)),
            Operator::NotEqual => Ok(!self.values_equal(left, right)),
            Operator::GreaterThan => self.compare_numbers(left, right, |a, b| a > b),
            Operator::GreaterThanOrEqual => self.compare_numbers(left, right, |a, b| a >= b),
            Operator::LessThan => self.compare_numbers(left, right, |a, b| a < b),
            Operator::LessThanOrEqual => self.compare_numbers(left, right, |a, b| a <= b),
            Operator::In => self.evaluate_in_operator(left, right),
            Operator::NotIn => Ok(!self.evaluate_in_operator(left, right)?),
            Operator::Contains => self.evaluate_contains_operator(left, right),
            Operator::StartsWith => self.evaluate_starts_with_operator(left, right),
            Operator::EndsWith => self.evaluate_ends_with_operator(left, right),
            Operator::Matches => self.evaluate_matches_operator(left, right),
        }
    }

    /// Check if two values are equal
    fn values_equal(&self, left: &AttributeValue, right: &AttributeValue) -> bool {
        match (left, right) {
            (AttributeValue::String(a), AttributeValue::String(b)) => a == b,
            (AttributeValue::Number(a), AttributeValue::Number(b)) => (a - b).abs() < f64::EPSILON,
            (AttributeValue::Boolean(a), AttributeValue::Boolean(b)) => a == b,
            (AttributeValue::Null, AttributeValue::Null) => true,
            // Try string conversion for mixed types
            _ => {
                if let (Some(a), Some(b)) = (left.as_string(), right.as_string()) {
                    a == b
                } else {
                    false
                }
            }
        }
    }

    /// Compare numeric values
    fn compare_numbers<F>(
        &self,
        left: &AttributeValue,
        right: &AttributeValue,
        comparator: F,
    ) -> Result<bool, AppError>
    where
        F: Fn(f64, f64) -> bool,
    {
        let left_num = left.as_number().ok_or_else(|| {
            AppError::BadRequest(format!("Cannot convert '{}' to number", left.type_name()))
        })?;
        let right_num = right.as_number().ok_or_else(|| {
            AppError::BadRequest(format!("Cannot convert '{}' to number", right.type_name()))
        })?;

        Ok(comparator(left_num, right_num))
    }

    /// Evaluate 'in' operator
    fn evaluate_in_operator(
        &self,
        left: &AttributeValue,
        right: &AttributeValue,
    ) -> Result<bool, AppError> {
        match right {
            AttributeValue::Array(array) => {
                Ok(array.iter().any(|item| self.values_equal(left, item)))
            }
            _ => Err(AppError::BadRequest(
                "'in' operator requires array on right side".to_string(),
            )),
        }
    }

    /// Evaluate 'contains' operator
    fn evaluate_contains_operator(
        &self,
        left: &AttributeValue,
        right: &AttributeValue,
    ) -> Result<bool, AppError> {
        match left {
            AttributeValue::String(haystack) => {
                if let Some(needle) = right.as_string() {
                    Ok(haystack.contains(&needle))
                } else {
                    Err(AppError::BadRequest(
                        "'contains' operator requires string values".to_string(),
                    ))
                }
            }
            AttributeValue::Array(array) => {
                Ok(array.iter().any(|item| self.values_equal(item, right)))
            }
            _ => Err(AppError::BadRequest(
                "'contains' operator requires string or array on left side".to_string(),
            )),
        }
    }

    /// Evaluate 'starts_with' operator
    fn evaluate_starts_with_operator(
        &self,
        left: &AttributeValue,
        right: &AttributeValue,
    ) -> Result<bool, AppError> {
        let left_str = left.as_string().ok_or_else(|| {
            AppError::BadRequest("'starts_with' operator requires string on left side".to_string())
        })?;
        let right_str = right.as_string().ok_or_else(|| {
            AppError::BadRequest("'starts_with' operator requires string on right side".to_string())
        })?;

        Ok(left_str.starts_with(&right_str))
    }

    /// Evaluate 'ends_with' operator
    fn evaluate_ends_with_operator(
        &self,
        left: &AttributeValue,
        right: &AttributeValue,
    ) -> Result<bool, AppError> {
        let left_str = left.as_string().ok_or_else(|| {
            AppError::BadRequest("'ends_with' operator requires string on left side".to_string())
        })?;
        let right_str = right.as_string().ok_or_else(|| {
            AppError::BadRequest("'ends_with' operator requires string on right side".to_string())
        })?;

        Ok(left_str.ends_with(&right_str))
    }

    /// Evaluate 'matches' operator (regex)
    fn evaluate_matches_operator(
        &self,
        left: &AttributeValue,
        right: &AttributeValue,
    ) -> Result<bool, AppError> {
        let left_str = left.as_string().ok_or_else(|| {
            AppError::BadRequest("'matches' operator requires string on left side".to_string())
        })?;
        let pattern = right.as_string().ok_or_else(|| {
            AppError::BadRequest(
                "'matches' operator requires string pattern on right side".to_string(),
            )
        })?;

        let regex = Regex::new(&pattern)
            .map_err(|e| AppError::BadRequest(format!("Invalid regex pattern: {}", e)))?;

        Ok(regex.is_match(&left_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::entities::abac::*;

    #[test]
    fn test_operator_evaluator_equal() {
        let evaluator = OperatorEvaluator::new();

        let left = AttributeValue::String("test".to_string());
        let right = AttributeValue::String("test".to_string());
        assert!(evaluator
            .evaluate_operator(Operator::Equal, &left, &right)
            .unwrap());

        let left = AttributeValue::Number(42.0);
        let right = AttributeValue::Number(42.0);
        assert!(evaluator
            .evaluate_operator(Operator::Equal, &left, &right)
            .unwrap());
    }

    #[test]
    fn test_operator_evaluator_greater_than() {
        let evaluator = OperatorEvaluator::new();

        let left = AttributeValue::Number(10.0);
        let right = AttributeValue::Number(5.0);
        assert!(evaluator
            .evaluate_operator(Operator::GreaterThan, &left, &right)
            .unwrap());

        let left = AttributeValue::Number(5.0);
        let right = AttributeValue::Number(10.0);
        assert!(!evaluator
            .evaluate_operator(Operator::GreaterThan, &left, &right)
            .unwrap());
    }

    #[test]
    fn test_operator_evaluator_in() {
        let evaluator = OperatorEvaluator::new();

        let left = AttributeValue::String("admin".to_string());
        let right = AttributeValue::Array(vec![
            AttributeValue::String("admin".to_string()),
            AttributeValue::String("user".to_string()),
        ]);
        assert!(evaluator
            .evaluate_operator(Operator::In, &left, &right)
            .unwrap());

        let left = AttributeValue::String("guest".to_string());
        assert!(!evaluator
            .evaluate_operator(Operator::In, &left, &right)
            .unwrap());
    }
}
