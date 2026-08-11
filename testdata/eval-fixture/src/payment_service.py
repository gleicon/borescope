"""PaymentService — the central class for this eval fixture.

The eval task: rename `PaymentService.charge` to `PaymentService.process`
and update all callers. There are callers in checkout.py, subscription.py,
and billing.py.
"""
import logging
import uuid

from .gateway import GatewayClient, GatewayError
from .models import Card, Money, PaymentIntent, PaymentResult, PaymentStatus, RefundResult
from .validation import validate_card, validate_amount
from .audit import AuditLogger

logger = logging.getLogger(__name__)


class PaymentError(Exception):
    def __init__(self, code: str, message: str, original: Exception = None):
        self.code = code
        self.message = message
        self.original = original
        super().__init__(message)


class PaymentService:
    """Orchestrates authorization, capture, and refund flows."""

    def __init__(
        self,
        gateway: GatewayClient,
        audit: AuditLogger,
        max_retries: int = 2,
    ):
        self._gateway = gateway
        self._audit = audit
        self._max_retries = max_retries

    # ------------------------------------------------------------------ #
    # Public API                                                           #
    # ------------------------------------------------------------------ #

    def charge(
        self,
        card: Card,
        amount: Money,
        merchant_id: str,
        idempotency_key: str = None,
        metadata: dict = None,
    ) -> PaymentResult:
        """Authorize and immediately capture a payment.

        This is the method to rename to `process` in the eval task.
        """
        if idempotency_key is None:
            idempotency_key = str(uuid.uuid4())

        intent = PaymentIntent(
            id=str(uuid.uuid4()),
            amount=amount,
            card=card,
            merchant_id=merchant_id,
            idempotency_key=idempotency_key,
            metadata=metadata or {},
        )

        validate_card(card)
        validate_amount(amount)

        self._audit.log_intent(intent)

        result = self._authorize_and_capture(intent)
        self._audit.log_result(result)
        return result

    def refund(self, transaction_id: str, amount: Money) -> RefundResult:
        """Issue a partial or full refund for a captured transaction."""
        validate_amount(amount)
        self._audit.log_refund_attempt(transaction_id, amount)
        raw = self._gateway.refund(transaction_id, amount)
        result = self._gateway.build_refund_result(transaction_id, amount, raw)
        self._audit.log_refund_result(result)
        return result

    def void(self, transaction_id: str) -> bool:
        """Void an authorized but not yet captured transaction."""
        raw = self._gateway.void(transaction_id)
        ok = raw.get("status") == "success"
        self._audit.log_void(transaction_id, ok)
        return ok

    # ------------------------------------------------------------------ #
    # Internal helpers                                                     #
    # ------------------------------------------------------------------ #

    def _authorize_and_capture(self, intent: PaymentIntent) -> PaymentResult:
        last_error = None
        for attempt in range(self._max_retries + 1):
            try:
                raw = self._gateway.authorize(
                    intent.card,
                    intent.amount,
                    intent.idempotency_key,
                    intent.metadata,
                )
                result = self._gateway.build_payment_result(intent.id, raw)
                if result.succeeded:
                    self._capture(result.transaction_id, intent.amount)
                return result
            except GatewayError as e:
                last_error = e
                logger.warning(
                    "Gateway error on attempt %d/%d: [%s] %s",
                    attempt + 1,
                    self._max_retries + 1,
                    e.code,
                    e.message,
                )
                if e.code in ("card_expired", "card_declined"):
                    break  # no point retrying hard declines
        raise PaymentError(
            code=last_error.code if last_error else "unknown",
            message=last_error.message if last_error else "Payment failed",
            original=last_error,
        )

    def _capture(self, transaction_id: str, amount: Money) -> None:
        raw = self._gateway.capture(transaction_id, amount)
        if raw.get("status") != "success":
            raise PaymentError(
                code="capture_failed",
                message=f"Capture failed for txn {transaction_id}",
            )
