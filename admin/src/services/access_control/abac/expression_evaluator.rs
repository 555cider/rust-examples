//! Expression Evaluator for ABAC Policies
//!
//! This module is responsible for parsing and evaluating simple expressions
//! found in ABAC policy targets and conditions. It provides a safe, simple,
//! and extensible way to check attributes against literal values.
//!
//! ## Key Components
//!
//! * `ExpressionEvaluator`: The main struct that performs the evaluation.
//! * `Operator`: An enum representing supported comparison operators.
//! * `Value`: An enum representing different types of values (String, Number, Bool).
//!
//! ## Example Usage
//!
//! ```rust
//! // Example coming soon
//! ```

use std::collections::HashMap;
use serde_json::Value as JsonValue;
use thiserror::Error;

/// Errors that can occur during expression evaluation.
#[derive(Debug, Error)]
pub enum ExpressionError {
    #[error("Invalid operator: {0}")]
    InvalidOperator(String),

    #[error("Type mismatch: expected {expected}, found {found}")]
    TypeMismatch {
        expected: String,
        found: String,
    },

    #[error("Attribute not found in context: {0}")]
    AttributeNotFound(String),

    #[error("Invalid value format: {0}")]
    InvalidValue(String),
}




use crate::models::entities::abac::{ConditionExpression, Operator};

/// The main evaluator for simple expressions.
#[derive(Clone)]
pub struct ExpressionEvaluator;

impl ExpressionEvaluator {
    pub fn new() -> Self {
        Self
    }

    /// Evaluates a simple expression (attribute op value) against the context.
    pub fn evaluate(
        &self,
        attribute: &str,
        operator: &Operator,
        value: &JsonValue,
        context: &HashMap<String, JsonValue>,
    ) -> Result<bool, ExpressionError> {
         let attribute_value = context
            .get(attribute)
            .ok_or_else(|| ExpressionError::AttributeNotFound(attribute.to_string()))?;

        self.compare(attribute_value, operator, value)
    }

    /// Evaluates a condition expression recursively.
    pub fn evaluate_condition(
        &self,
        expression: &ConditionExpression,
        context: &HashMap<String, JsonValue>,
    ) -> Result<bool, ExpressionError> {
        match expression {
            ConditionExpression::Simple { attribute, operator, value } => {
                let attribute_value = context
                    .get(attribute)
                    .ok_or_else(|| ExpressionError::AttributeNotFound(attribute.to_string()))?;

                let expected_value: JsonValue = value.clone().into();

                self.compare(attribute_value, operator, &expected_value)
            }
            ConditionExpression::And { and } => {
                for cond in and {
                    if !self.evaluate_condition(&cond.expression, context)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            ConditionExpression::Or { or } => {
                for cond in or {
                    if self.evaluate_condition(&cond.expression, context)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            ConditionExpression::Not { not } => {
                Ok(!self.evaluate_condition(&not.expression, context)?)
            }
        }
    }

    /// Compares two JSON values based on the operator.
    fn compare(
        &self,
        actual: &JsonValue,
        op: &Operator,
        expected: &JsonValue,
    ) -> Result<bool, ExpressionError> {
        match (actual, expected) {
            // String comparisons
            (JsonValue::String(actual_str), JsonValue::String(expected_str)) => match op {
                Operator::Equal => Ok(actual_str == expected_str),
                Operator::NotEqual => Ok(actual_str != expected_str),
                Operator::StartsWith => Ok(actual_str.starts_with(expected_str)),
                Operator::EndsWith => Ok(actual_str.ends_with(expected_str)),
                Operator::Contains => Ok(actual_str.contains(expected_str)),
                _ => Err(ExpressionError::InvalidOperator("Not a string operator".to_string())),
            },
            // Number comparisons
            (JsonValue::Number(actual_num), JsonValue::Number(expected_num)) => {
                let actual_f64 = actual_num.as_f64().unwrap();
                let expected_f64 = expected_num.as_f64().unwrap();
                match op {
                    Operator::Equal => Ok(actual_f64 == expected_f64),
                    Operator::NotEqual => Ok(actual_f64 != expected_f64),
                    Operator::GreaterThan => Ok(actual_f64 > expected_f64),
                    Operator::LessThan => Ok(actual_f64 < expected_f64),
                    Operator::GreaterThanOrEqual => Ok(actual_f64 >= expected_f64),
                    Operator::LessThanOrEqual => Ok(actual_f64 <= expected_f64),
                    _ => Err(ExpressionError::InvalidOperator("Not a number operator".to_string())),
                }
            }
            // Boolean comparisons
            (JsonValue::Bool(actual_bool), JsonValue::Bool(expected_bool)) => match op {
                Operator::Equal => Ok(actual_bool == expected_bool),
                Operator::NotEqual => Ok(actual_bool != expected_bool),
                _ => Err(ExpressionError::InvalidOperator("Not a boolean operator".to_string())),
            },
            // Array 'in'/'not_in' check
            (val, JsonValue::Array(arr)) => match op {
                Operator::In => Ok(arr.contains(val)),
                Operator::NotIn => Ok(!arr.contains(val)),
                _ => Err(ExpressionError::InvalidOperator("Not an array operator".to_string())),
            },
            // Type mismatch
            _ => Err(ExpressionError::TypeMismatch {
                expected: format_type(expected),
                found: format_type(actual),
            }),
        }
    }
}

/// Helper to format serde_json::Value types for error messages.
fn format_type(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(_) => "boolean".to_string(),
        JsonValue::Number(_) => "number".to_string(),
        JsonValue::String(_) => "string".to_string(),
        JsonValue::Array(_) => "array".to_string(),
        JsonValue::Object(_) => "object".to_string(),
    }
}

// Unit tests
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_context() -> HashMap<String, JsonValue> {
        let mut context = HashMap::new();
        context.insert("user.role".to_string(), json!("admin"));
        context.insert("request.ip".to_string(), json!("192.168.1.10"));
        context.insert("resource.owner_id".to_string(), json!(123));
        context.insert("environment.is_prod".to_string(), json!(true));
        context
    }

    #[test]
    fn test_string_equals_success() {
        let evaluator = ExpressionEvaluator::new();
        let context = create_test_context();
        let result = evaluator.evaluate("user.role", &Operator::Equal, &json!("admin"), &context);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_string_not_equals_success() {
        let evaluator = ExpressionEvaluator::new();
        let context = create_test_context();
        let result = evaluator.evaluate("user.role", &Operator::NotEqual, &json!("guest"), &context);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_number_greater_than_success() {
        let evaluator = ExpressionEvaluator::new();
        let context = create_test_context();
        let result = evaluator.evaluate("resource.owner_id", &Operator::GreaterThan, &json!(100), &context);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_boolean_equals_success() {
        let evaluator = ExpressionEvaluator::new();
        let context = create_test_context();
        let result = evaluator.evaluate("environment.is_prod", &Operator::Equal, &json!(true), &context);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_attribute_not_found() {
        let evaluator = ExpressionEvaluator::new();
        let context = create_test_context();
        let result = evaluator.evaluate("user.department", &Operator::Equal, &json!("IT"), &context);
        assert!(result.is_err());
        match result.err().unwrap() {
            ExpressionError::AttributeNotFound(attr) => assert_eq!(attr, "user.department"),
            _ => panic!("Expected AttributeNotFound error"),
        }
    }

    #[test]
    fn test_type_mismatch() {
        let evaluator = ExpressionEvaluator::new();
        let context = create_test_context();
        // Comparing a number with a string
        let result = evaluator.evaluate("resource.owner_id", &Operator::Equal, &json!("123"), &context);
        assert!(result.is_err());
        match result.err().unwrap() {
            ExpressionError::TypeMismatch { expected, found } => {
                assert_eq!(expected, "string");
                assert_eq!(found, "number");
            }
            _ => panic!("Expected TypeMismatch error"),
        }
    }

    #[test]
    fn test_string_contains_success() {
        let evaluator = ExpressionEvaluator::new();
        let context = create_test_context();
        let result = evaluator.evaluate("request.ip", &Operator::Contains, &json!("168.1"), &context);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }
}
