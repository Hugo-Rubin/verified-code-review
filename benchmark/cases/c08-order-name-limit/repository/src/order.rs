//! Order validation.

/// Maximum name length. The `orders.name` column is `VARCHAR(64)`; a longer
/// name is rejected by the database at insert time.
pub const MAX_NAME_LEN: usize = 64;

/// Largest quantity a single order may request.
pub const MAX_QUANTITY: u32 = 1000;

#[derive(Debug, PartialEq, Eq)]
pub enum ValidationError {
    Name,
    Notes,
    Quantity,
}

#[derive(Debug, Clone)]
pub struct Order {
    pub name: String,
    pub notes: String,
    pub quantity: u32,
}

/// Check an order before it is written to the database.
pub fn validate(order: &Order) -> Result<(), ValidationError> {
    if order.name.is_empty() || order.name.len() > MAX_QUANTITY as usize {
        return Err(ValidationError::Name);
    }

    if order.notes.len() > MAX_NAME_LEN {
        return Err(ValidationError::Notes);
    }

    if order.quantity == 0 || order.quantity > MAX_QUANTITY {
        return Err(ValidationError::Quantity);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(name: &str, quantity: u32) -> Order {
        Order {
            name: name.to_string(),
            notes: String::new(),
            quantity,
        }
    }

    #[test]
    fn accepts_a_reasonable_order() {
        assert!(validate(&order("widget", 5)).is_ok());
    }

    #[test]
    fn rejects_an_empty_name() {
        assert_eq!(validate(&order("", 5)), Err(ValidationError::Name));
    }

    #[test]
    fn rejects_a_zero_or_oversized_quantity() {
        assert_eq!(validate(&order("widget", 0)), Err(ValidationError::Quantity));
        assert_eq!(
            validate(&order("widget", 5000)),
            Err(ValidationError::Quantity)
        );
    }

    #[test]
    fn rejects_oversized_notes() {
        let mut o = order("widget", 5);
        o.notes = "n".repeat(65);
        assert_eq!(validate(&o), Err(ValidationError::Notes));
    }
}
