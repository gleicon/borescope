"""Input validation utilities."""
import re
from .models import Card, Money, Currency


class ValidationError(Exception):
    def __init__(self, field: str, message: str):
        self.field = field
        self.message = message
        super().__init__(f"{field}: {message}")


def validate_card(card: Card) -> None:
    """Raise ValidationError if card fields are invalid."""
    if not card.token or len(card.token) < 8:
        raise ValidationError("card.token", "Token must be at least 8 characters")
    if card.is_expired():
        raise ValidationError(
            "card.expiry",
            f"Card ending {card.last4} expired {card.expiry_month:02d}/{card.expiry_year}",
        )
    if not re.fullmatch(r"\d{4}", card.last4):
        raise ValidationError("card.last4", "last4 must be exactly 4 digits")
    if not (1 <= card.expiry_month <= 12):
        raise ValidationError("card.expiry_month", "Month must be 1–12")
    if len(card.cardholder.strip()) < 2:
        raise ValidationError("card.cardholder", "Cardholder name too short")


def validate_amount(amount: Money) -> None:
    """Raise ValidationError if amount is invalid."""
    if amount.amount <= 0:
        raise ValidationError("amount", "Amount must be positive")
    if amount.amount > 100_000_00:  # $100,000 hard ceiling
        raise ValidationError("amount", "Amount exceeds maximum allowed")
    if amount.currency not in Currency:
        raise ValidationError("amount.currency", f"Unsupported currency {amount.currency}")


def validate_merchant_id(merchant_id: str) -> None:
    if not merchant_id or not re.fullmatch(r"[a-z0-9_-]{4,64}", merchant_id):
        raise ValidationError(
            "merchant_id",
            "Merchant ID must be 4–64 characters, lowercase alphanumeric/dash/underscore",
        )


def validate_idempotency_key(key: str) -> None:
    if not key or len(key) > 255:
        raise ValidationError(
            "idempotency_key",
            "Idempotency key must be 1–255 characters",
        )
