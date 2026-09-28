import { config } from "../lib/config";

export function applyTax(amount: number): number {
  if (amount <= 0) {
    return 0;
  }
  return amount * (1 + config.taxRate);
}
