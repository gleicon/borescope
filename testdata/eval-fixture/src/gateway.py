"""Payment gateway adapter — wraps external processor API."""
import hashlib
import time
import uuid
from typing import Optional

from .models import Card, Money, PaymentResult, PaymentStatus, RefundResult


class GatewayError(Exception):
    def __init__(self, code: str, message: str):
        self.code = code
        self.message = message
        super().__init__(f"[{code}] {message}")


class GatewayClient:
    """Thin wrapper around the external payment processor HTTP API."""

    def __init__(self, api_key: str, endpoint: str = "https://api.processor.example"):
        self._api_key = api_key
        self._endpoint = endpoint
        self._timeout = 10

    def _sign_request(self, payload: dict) -> str:
        raw = "&".join(f"{k}={v}" for k, v in sorted(payload.items()))
        return hashlib.sha256(f"{self._api_key}:{raw}".encode()).hexdigest()

    def _post(self, path: str, payload: dict) -> dict:
        """Simulate HTTP POST; in production this calls the real endpoint."""
        # Deterministic simulation: token "fail_*" always fails.
        if payload.get("card_token", "").startswith("fail_"):
            return {
                "status": "failed",
                "error_code": "card_declined",
                "error_message": "Card was declined",
            }
        txn_id = "txn_" + hashlib.md5(
            (payload.get("idempotency_key", "") + path).encode()
        ).hexdigest()[:12]
        return {"status": "success", "transaction_id": txn_id}

    def authorize(
        self,
        card: Card,
        amount: Money,
        idempotency_key: str,
        metadata: Optional[dict] = None,
    ) -> dict:
        if card.is_expired():
            raise GatewayError("card_expired", f"Card ending {card.last4} is expired")
        payload = {
            "card_token": card.token,
            "amount": amount.amount,
            "currency": amount.currency.value,
            "idempotency_key": idempotency_key,
        }
        if metadata:
            payload.update({f"meta_{k}": str(v) for k, v in metadata.items()})
        return self._post("/v1/authorize", payload)

    def capture(self, transaction_id: str, amount: Money) -> dict:
        return self._post(
            "/v1/capture",
            {"transaction_id": transaction_id, "amount": amount.amount},
        )

    def void(self, transaction_id: str) -> dict:
        return self._post("/v1/void", {"transaction_id": transaction_id})

    def refund(self, transaction_id: str, amount: Money) -> dict:
        payload = {
            "transaction_id": transaction_id,
            "amount": amount.amount,
            "currency": amount.currency.value,
        }
        result = self._post("/v1/refund", payload)
        return result

    def build_payment_result(self, intent_id: str, raw: dict) -> PaymentResult:
        if raw.get("status") == "success":
            return PaymentResult(
                intent_id=intent_id,
                status=PaymentStatus.SUCCESS,
                transaction_id=raw.get("transaction_id"),
            )
        return PaymentResult(
            intent_id=intent_id,
            status=PaymentStatus.FAILED,
            error_code=raw.get("error_code"),
            error_message=raw.get("error_message"),
        )

    def build_refund_result(self, txn_id: str, amount: Money, raw: dict) -> RefundResult:
        refund_id = "ref_" + str(uuid.uuid4())[:8]
        status = PaymentStatus.REFUNDED if raw.get("status") == "success" else PaymentStatus.FAILED
        return RefundResult(
            original_transaction_id=txn_id,
            refund_id=refund_id,
            amount=amount,
            status=status,
        )
