use super::*;

/// Condition evaluator for processing policy conditions
#[derive(Clone)]
pub struct ConditionEvaluator {
    operator_evaluator: OperatorEvaluator,
}

impl ConditionEvaluator {
    pub fn new() -> Self {
        Self {
            operator_evaluator: OperatorEvaluator::new(),
        }
    }

    /// Evaluate a condition expression
    pub fn evaluate_condition(
        &self,
        condition: &Condition,
        attributes: &HashMap<String, AttributeValue>,
    ) -> Result<bool, AppError> {
        self.evaluate_condition_expression(&condition.expression, attributes)
    }

    /// Evaluate condition expression recursively
    fn evaluate_condition_expression(
        &self,
        expression: &ConditionExpression,
        attributes: &HashMap<String, AttributeValue>,
    ) -> Result<bool, AppError> {
        match expression {
            ConditionExpression::Simple {
                attribute,
                operator,
                value,
            } => {
                let attr_value = attributes.get(attribute).ok_or_else(|| {
                    AppError::BadRequest(format!("Attribute '{}' not found", attribute))
                })?;

                self.operator_evaluator
                    .evaluate_operator(*operator, attr_value, value)
            }
            ConditionExpression::And { and } => {
                for condition in and {
                    if !self.evaluate_condition_expression(&condition.expression, attributes)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            ConditionExpression::Or { or } => {
                for condition in or {
                    if self.evaluate_condition_expression(&condition.expression, attributes)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            ConditionExpression::Not { not } => {
                let result = self.evaluate_condition_expression(&not.expression, attributes)?;
                Ok(!result)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::entities::abac::*;

    #[test]
    fn test_condition_evaluator_and() {
        let evaluator = ConditionEvaluator::new();
        let mut attributes = HashMap::new();
        attributes.insert(
            "user.department".to_string(),
            AttributeValue::String("IT".to_string()),
        );
        attributes.insert("user.level".to_string(), AttributeValue::Number(5.0));

        let condition = Condition {
            expression: ConditionExpression::And {
                and: vec![
                    Condition {
                        expression: ConditionExpression::Simple {
                            attribute: "user.department".to_string(),
                            operator: Operator::Equal,
                            value: AttributeValue::String("IT".to_string()),
                        },
                    },
                    Condition {
                        expression: ConditionExpression::Simple {
                            attribute: "user.level".to_string(),
                            operator: Operator::GreaterThan,
                            value: AttributeValue::Number(3.0),
                        },
                    },
                ],
            },
        };

        assert!(evaluator
            .evaluate_condition(&condition, &attributes)
            .unwrap());
    }

    #[test]
    fn test_condition_evaluator_or() {
        let evaluator = ConditionEvaluator::new();
        let mut attributes = HashMap::new();
        attributes.insert(
            "user.department".to_string(),
            AttributeValue::String("HR".to_string()),
        );
        attributes.insert("user.level".to_string(), AttributeValue::Number(5.0));

        let condition = Condition {
            expression: ConditionExpression::Or {
                or: vec![
                    Condition {
                        expression: ConditionExpression::Simple {
                            attribute: "user.department".to_string(),
                            operator: Operator::Equal,
                            value: AttributeValue::String("IT".to_string()),
                        },
                    },
                    Condition {
                        expression: ConditionExpression::Simple {
                            attribute: "user.level".to_string(),
                            operator: Operator::GreaterThan,
                            value: AttributeValue::Number(3.0),
                        },
                    },
                ],
            },
        };

        assert!(evaluator
            .evaluate_condition(&condition, &attributes)
            .unwrap());
    }

    #[test]
    fn test_condition_evaluator_not() {
        let evaluator = ConditionEvaluator::new();
        let mut attributes = HashMap::new();
        attributes.insert(
            "user.department".to_string(),
            AttributeValue::String("HR".to_string()),
        );

        let condition = Condition {
            expression: ConditionExpression::Not {
                not: Box::new(Condition {
                    expression: ConditionExpression::Simple {
                        attribute: "user.department".to_string(),
                        operator: Operator::Equal,
                        value: AttributeValue::String("IT".to_string()),
                    },
                }),
            },
        };

        assert!(evaluator
            .evaluate_condition(&condition, &attributes)
            .unwrap());
    }
}
