"""Checkout flow — caller 1 of PaymentService.charge (eval rename target)."""
import logging
import uuid

from .models import Card, Currency, Money, Order, PaymentResult
from .payment_service import PaymentService, PaymentError

logger = logging.getLogger(__name__)


class CheckoutError(Exception):
    pass


class CheckoutService:
    """Drives the end-to-end checkout flow: validate cart → reserve inventory → pay."""

    def __init__(self, payment_service: PaymentService):
        self._payments = payment_service

    def complete_order(self, order: Order, card: Card) -> PaymentResult:
        """Finalize an order by charging the customer's card.

        This is CALLER 1 of PaymentService.charge — must be updated in the eval task.
        """
        self._validate_order(order)
        result = self._payments.charge(
            card=card,
            amount=order.total,
            merchant_id="merchant_main",
            idempotency_key=f"order_{order.id}",
            metadata={"order_id": order.id, "customer_id": order.customer_id},
        )
        if not result.succeeded:
            raise CheckoutError(
                f"Payment failed for order {order.id}: {result.error_message}"
            )
        logger.info("Order %s paid — txn %s", order.id, result.transaction_id)
        return result

    def refund_order(self, order: Order, transaction_id: str) -> None:
        """Refund the full order amount."""
        self._payments.refund(transaction_id, order.total)
        logger.info("Refunded order %s (txn %s)", order.id, transaction_id)

    def _validate_order(self, order: Order) -> None:
        if not order.items:
            raise CheckoutError(f"Order {order.id} has no items")
        if order.total.amount <= 0:
            raise CheckoutError(f"Order {order.id} has non-positive total")

    def quick_charge(
        self,
        card: Card,
        amount_cents: int,
        currency: Currency,
        merchant_id: str,
    ) -> PaymentResult:
        """One-off charge without a full order object.

        Also calls PaymentService.charge — second call site in this file.
        """
        amount = Money(amount_cents, currency)
        return self._payments.charge(
            card=card,
            amount=amount,
            merchant_id=merchant_id,
            idempotency_key=str(uuid.uuid4()),
        )
