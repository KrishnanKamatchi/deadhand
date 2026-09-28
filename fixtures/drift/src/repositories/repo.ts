export const repo = {
  find: (table: string, id: string) => ({ table, id }),
  all: (table: string) => [table],
};
