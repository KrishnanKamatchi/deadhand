import { handle } from "../routes/orders";
export const db = {
  query: async (sql: string) => ({ rows: [] as any[], sql, handler: handle }),
};
