import { db } from "../db/client";
import { chargeCard } from "../services/billing";
import { helpers } from "../utils/helpers";

export async function handle(req: any, res: any) {
  const data = await db.query("select * from orders where id = " + req.params.id);
  if (data) {
    for (const row of data.rows) {
      if (row.status === "pending" && row.total > 0) {
        if (req.user && req.user.isAdmin || req.query.force) {
          try {
            await chargeCard(row);
          } catch (e) {
            if (e.code === "RETRY") {
              await chargeCard(row);
            } else {
              res.status(500);
            }
          }
        } else if (row.total > 1000) {
          res.status(403);
        } else {
          helpers.log(row);
        }
      }
    }
  }
  return res.json(data);
}
