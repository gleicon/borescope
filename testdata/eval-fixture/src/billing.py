"""Admin billing utilities — caller 3 of PaymentService.charge (eval rename target)."""
import logging
from typing import List, Tuple

from .models import Card, Currency, Money, PaymentResult
from .payment_service import PaymentService, PaymentError
from .validation import validate_amount

logger = logging.getLogger(__name__)


class BillingReport:
    def __init__(self):
        self.succeeded: list[PaymentResult] = []
        self.failed: list[Tuple[str, str]] = []  # (label, error_message)

    @property
    def total_charged(self) -> int:
        return sum(1 for _ in self.succeeded)

    @property
    def total_failed(self) -> int:
        return len(self.failed)

    def summary(self) -> str:
        return (
            f"Billing complete: {self.total_charged} succeeded, "
            f"{self.total_failed} failed"
        )


class BillingService:
    """Batch billing operations for admin workflows."""

    def __init__(self, payment_service: PaymentService):
        self._payments = payment_service

    def batch_charge(
        self,
        items: List[Tuple[str, Card, Money]],  # (label, card, amount)
        merchant_id: str,
    ) -> BillingReport:
        """Charge a list of (label, card, amount) tuples.

        This is CALLER 3 of PaymentService.charge — must be updated in the eval task.
        """
        report = BillingReport()
        for label, card, amount in items:
            try:
                validate_amount(amount)
                result = self._payments.charge(
                    card=card,
                    amount=amount,
                    merchant_id=merchant_id,
                    metadata={"batch_label": label},
                )
                if result.succeeded:
                    report.succeeded.append(result)
                    logger.info("Batch charge OK: %s txn=%s", label, result.transaction_id)
                else:
                    report.failed.append((label, result.error_message or "unknown"))
                    logger.warning("Batch charge failed: %s — %s", label, result.error_message)
            except PaymentError as e:
                report.failed.append((label, e.message))
                logger.error("Batch charge error: %s — %s", label, e.message)
        return report

    def invoice_charge(
        self,
        invoice_id: str,
        card: Card,
        line_items: List[Tuple[str, int]],  # (description, cents)
        currency: Currency,
        merchant_id: str,
    ) -> PaymentResult:
        """Charge a single card for the sum of all invoice line items.

        Also calls PaymentService.charge — second call site in this file.
        """
        total = sum(cents for _, cents in line_items)
        amount = Money(total, currency)
        validate_amount(amount)
        return self._payments.charge(
            card=card,
            amount=amount,
            merchant_id=merchant_id,
            idempotency_key=f"invoice_{invoice_id}",
            metadata={"invoice_id": invoice_id, "line_items": len(line_items)},
        )

    def split_charge(
        self,
        cards: List[Tuple[Card, Money]],  # (card, portion)
        merchant_id: str,
        invoice_id: str,
    ) -> BillingReport:
        """Split a charge across multiple cards (e.g., corporate split billing).

        Third call site in this file.
        """
        report = BillingReport()
        for i, (card, portion) in enumerate(cards):
            label = f"split_{invoice_id}_{i}"
            try:
                result = self._payments.charge(
                    card=card,
                    amount=portion,
                    merchant_id=merchant_id,
                    idempotency_key=label,
                    metadata={"split_index": i, "invoice_id": invoice_id},
                )
                if result.succeeded:
                    report.succeeded.append(result)
                else:
                    report.failed.append((label, result.error_message or "unknown"))
            except PaymentError as e:
                report.failed.append((label, e.message))
        return report
