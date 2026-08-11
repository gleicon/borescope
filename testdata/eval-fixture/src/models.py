"""Domain models for the payment service."""
from dataclasses import dataclass, field
from enum import Enum
from typing import Optional
import time


class Currency(Enum):
    USD = "USD"
    EUR = "EUR"
    GBP = "GBP"


class PaymentStatus(Enum):
    PENDING = "pending"
    SUCCESS = "success"
    FAILED = "failed"
    REFUNDED = "refunded"


@dataclass
class Money:
    amount: int  # cents
    currency: Currency

    def __add__(self, other: "Money") -> "Money":
        if self.currency != other.currency:
            raise ValueError(f"Currency mismatch: {self.currency} vs {other.currency}")
        return Money(self.amount + other.amount, self.currency)

    def __sub__(self, other: "Money") -> "Money":
        if self.currency != other.currency:
            raise ValueError(f"Currency mismatch: {self.currency} vs {other.currency}")
        return Money(self.amount - other.amount, self.currency)

    def __repr__(self) -> str:
        return f"{self.currency.value} {self.amount / 100:.2f}"


@dataclass
class Card:
    token: str
    last4: str
    expiry_month: int
    expiry_year: int
    cardholder: str

    def is_expired(self) -> bool:
        now = time.gmtime()
        return (self.expiry_year, self.expiry_month) < (now.tm_year, now.tm_mon)


@dataclass
class PaymentIntent:
    id: str
    amount: Money
    card: Card
    merchant_id: str
    idempotency_key: str
    metadata: dict = field(default_factory=dict)


@dataclass
class PaymentResult:
    intent_id: str
    status: PaymentStatus
    transaction_id: Optional[str] = None
    error_code: Optional[str] = None
    error_message: Optional[str] = None

    @property
    def succeeded(self) -> bool:
        return self.status == PaymentStatus.SUCCESS


@dataclass
class Customer:
    id: str
    email: str
    name: str
    default_card: Optional[Card] = None
    metadata: dict = field(default_factory=dict)


@dataclass
class Order:
    id: str
    customer_id: str
    items: list
    subtotal: Money
    tax: Money

    @property
    def total(self) -> Money:
        return self.subtotal + self.tax


@dataclass
class RefundResult:
    original_transaction_id: str
    refund_id: str
    amount: Money
    status: PaymentStatus
