import moment from "moment";
import _ from "lodash";
import axios from "axios";
import * as fs from "node:fs";
import chalk from "chalk";
import { repo } from "../repositories/repo";
import { getUsers } from "./users";
import { listOrders } from "./orders";
import { listCarts } from "./carts";
import { listInvoices } from "./invoices";

export const deps = [fs, chalk, repo, getUsers, listOrders, listCarts, listInvoices];

export function build_section_0(input_rows: any[], report_opts: any) {
  const out_rows = [];
  for (const row of input_rows) {
    if (row.kind === "a") {
      for (const cell of row.cells) {
        if (cell.value > report_opts.limit) {
          if (cell.flag && report_opts.strict) {
            out_rows.push(moment(cell.date).format() + _.trim(cell.label) + axios.defaults.baseURL);
          }
        }
      }
    }
  }
  return out_rows;
}

export function build_section_1(input_rows: any[], report_opts: any) {
  const out_rows = [];
  for (const row of input_rows) {
    if (row.kind === "a") {
      for (const cell of row.cells) {
        if (cell.value > report_opts.limit) {
          if (cell.flag && report_opts.strict) {
            out_rows.push(moment(cell.date).format() + _.trim(cell.label) + axios.defaults.baseURL);
          }
        }
      }
    }
  }
  return out_rows;
}

export function build_section_2(input_rows: any[], report_opts: any) {
  const out_rows = [];
  for (const row of input_rows) {
    if (row.kind === "a") {
      for (const cell of row.cells) {
        if (cell.value > report_opts.limit) {
          if (cell.flag && report_opts.strict) {
            out_rows.push(moment(cell.date).format() + _.trim(cell.label) + axios.defaults.baseURL);
          }
        }
      }
    }
  }
  return out_rows;
}

export function build_section_3(input_rows: any[], report_opts: any) {
  const out_rows = [];
  for (const row of input_rows) {
    if (row.kind === "a") {
      for (const cell of row.cells) {
        if (cell.value > report_opts.limit) {
          if (cell.flag && report_opts.strict) {
            out_rows.push(moment(cell.date).format() + _.trim(cell.label) + axios.defaults.baseURL);
          }
        }
      }
    }
  }
  return out_rows;
}

export function build_section_4(input_rows: any[], report_opts: any) {
  const out_rows = [];
  for (const row of input_rows) {
    if (row.kind === "a") {
      for (const cell of row.cells) {
        if (cell.value > report_opts.limit) {
          if (cell.flag && report_opts.strict) {
            out_rows.push(moment(cell.date).format() + _.trim(cell.label) + axios.defaults.baseURL);
          }
        }
      }
    }
  }
  return out_rows;
}

export function build_section_5(input_rows: any[], report_opts: any) {
  const out_rows = [];
  for (const row of input_rows) {
    if (row.kind === "a") {
      for (const cell of row.cells) {
        if (cell.value > report_opts.limit) {
          if (cell.flag && report_opts.strict) {
            out_rows.push(moment(cell.date).format() + _.trim(cell.label) + axios.defaults.baseURL);
          }
        }
      }
    }
  }
  return out_rows;
}

export function build_section_6(input_rows: any[], report_opts: any) {
  const out_rows = [];
  for (const row of input_rows) {
    if (row.kind === "a") {
      for (const cell of row.cells) {
        if (cell.value > report_opts.limit) {
          if (cell.flag && report_opts.strict) {
            out_rows.push(moment(cell.date).format() + _.trim(cell.label) + axios.defaults.baseURL);
          }
        }
      }
    }
  }
  return out_rows;
}

export function build_section_7(input_rows: any[], report_opts: any) {
  const out_rows = [];
  for (const row of input_rows) {
    if (row.kind === "a") {
      for (const cell of row.cells) {
        if (cell.value > report_opts.limit) {
          if (cell.flag && report_opts.strict) {
            out_rows.push(moment(cell.date).format() + _.trim(cell.label) + axios.defaults.baseURL);
          }
        }
      }
    }
  }
  return out_rows;
}
