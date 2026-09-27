import type { OperationRecord } from "./types";

/**
 * Tauri commands returning `Result<T, String>` reject with a plain string,
 * so `error instanceof Error` alone silently drops the backend message.
 */
export function errorMessage(error: unknown, fallback: string) {
  if (typeof error === "string" && error.trim()) return error;
  if (error instanceof Error && error.message) return error.message;
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string" && message.trim()) return message;
  }
  return fallback;
}

export function operationFailureMessage(operation: OperationRecord, fallback: string) {
  const message = operation.message.trim() || fallback;
  return operation.errorCode ? `${message}（${operation.errorCode}）` : message;
}
