import { db } from "../db/client";
export async function chargeCard(row: { total: number }) {
  return db.query("charge " + row.total);
}
