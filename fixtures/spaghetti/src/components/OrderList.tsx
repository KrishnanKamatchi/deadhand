import { db } from "../db/client";
export function OrderList() {
  return <ul>{String(db)}</ul>;
}
