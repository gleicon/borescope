"""Subscription billing — caller 2 of PaymentService.charge (eval rename target)."""
import logging
import time
from dataclasses import dataclass, field
from typing import Optional

from .models import Card, Currency, Money, PaymentResult, PaymentStatus
from .payment_service import PaymentService, PaymentError

logger = logging.getLogger(__name__)

BILLING_INTERVAL_DAYS = {
    "monthly": 30,
    "quarterly": 90,
    "annual": 365,
}


@dataclass
class Subscription:
    id: str
    customer_id: str
    plan: str                # "monthly" | "quarterly" | "annual"
    amount: Money
    card: Card
    merchant_id: str
    next_billing_ts: float
    active: bool = True
    failure_count: int = 0
    metadata: dict = field(default_factory=dict)


class SubscriptionService:
    """Manages recurring billing for active subscriptions."""

    MAX_CONSECUTIVE_FAILURES = 3

    def __init__(self, payment_service: PaymentService):
        self._payments = payment_service
        self._subscriptions: dict[str, Subscription] = {}

    def register(self, subscription: Subscription) -> None:
        self._subscriptions[subscription.id] = subscription

    def cancel(self, subscription_id: str) -> None:
        sub = self._subscriptions.get(subscription_id)
        if sub:
            sub.active = False
            logger.info("Subscription %s cancelled", subscription_id)

    def bill_due(self) -> list[str]:
        """Return IDs of subscriptions due for billing right now."""
        now = time.time()
        return [
            sid
            for sid, sub in self._subscriptions.items()
            if sub.active and sub.next_billing_ts <= now
        ]

    def run_billing_cycle(self) -> dict:
        """Attempt payment for every due subscription.

        This is CALLER 2 of PaymentService.charge — must be updated in the eval task.
        """
        due = self.bill_due()
        results = {"charged": [], "failed": [], "suspended": []}

        for sub_id in due:
            sub = self._subscriptions[sub_id]
            try:
                result = self._payments.charge(
                    card=sub.card,
                    amount=sub.amount,
                    merchant_id=sub.merchant_id,
                    idempotency_key=f"sub_{sub.id}_{int(sub.next_billing_ts)}",
                    metadata={"subscription_id": sub.id, "plan": sub.plan},
                )
                if result.succeeded:
                    sub.failure_count = 0
                    sub.next_billing_ts += (
                        BILLING_INTERVAL_DAYS[sub.plan] * 86400
                    )
                    results["charged"].append(sub_id)
                    logger.info("Billed sub %s — txn %s", sub_id, result.transaction_id)
                else:
                    self._handle_failure(sub, results)
            except PaymentError as e:
                logger.error("PaymentError on sub %s: %s", sub_id, e.message)
                self._handle_failure(sub, results)

        return results

    def _handle_failure(self, sub: Subscription, results: dict) -> None:
        sub.failure_count += 1
        if sub.failure_count >= self.MAX_CONSECUTIVE_FAILURES:
            sub.active = False
            results["suspended"].append(sub.id)
            logger.warning("Subscription %s suspended after %d failures", sub.id, sub.failure_count)
        else:
            results["failed"].append(sub.id)

    def update_card(self, subscription_id: str, new_card: Card) -> None:
        sub = self._subscriptions.get(subscription_id)
        if sub:
            sub.card = new_card
            logger.info("Card updated for sub %s", subscription_id)
