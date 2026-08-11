"""Audit logging — records every payment intent, result, and refund."""
import json
import logging
import time
from typing import Any

from .models import Money, PaymentIntent, PaymentResult, RefundResult

logger = logging.getLogger(__name__)


class AuditRecord:
    __slots__ = ("ts", "event", "data")

    def __init__(self, event: str, data: dict):
        self.ts = time.time()
        self.event = event
        self.data = data

    def to_dict(self) -> dict:
        return {"ts": self.ts, "event": self.event, **self.data}


class AuditLogger:
    """Append-only in-memory audit log. Swap for a DB-backed implementation in prod."""

    def __init__(self):
        self._records: list[AuditRecord] = []

    def _push(self, event: str, data: dict) -> None:
        rec = AuditRecord(event, data)
        self._records.append(rec)
        logger.debug("audit %s: %s", event, json.dumps(data))

    def log_intent(self, intent: PaymentIntent) -> None:
        self._push(
            "payment_intent",
            {
                "intent_id": intent.id,
                "merchant_id": intent.merchant_id,
                "amount": intent.amount.amount,
                "currency": intent.amount.currency.value,
                "card_last4": intent.card.last4,
                "idempotency_key": intent.idempotency_key,
            },
        )

    def log_result(self, result: PaymentResult) -> None:
        self._push(
            "payment_result",
            {
                "intent_id": result.intent_id,
                "status": result.status.value,
                "transaction_id": result.transaction_id,
                "error_code": result.error_code,
            },
        )

    def log_refund_attempt(self, transaction_id: str, amount: Money) -> None:
        self._push(
            "refund_attempt",
            {
                "transaction_id": transaction_id,
                "amount": amount.amount,
                "currency": amount.currency.value,
            },
        )

    def log_refund_result(self, result: RefundResult) -> None:
        self._push(
            "refund_result",
            {
                "refund_id": result.refund_id,
                "original_txn": result.original_transaction_id,
                "status": result.status.value,
            },
        )

    def log_void(self, transaction_id: str, success: bool) -> None:
        self._push("void", {"transaction_id": transaction_id, "success": success})

    def records(self) -> list[dict]:
        return [r.to_dict() for r in self._records]

    def clear(self) -> None:
        self._records.clear()
