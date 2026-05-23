/* Generated from src/const.rs. DO NOT EDIT. */

export const ROUTES = {
  CREATE: 1,
  JOIN: 2,
} as const;

export type RouteCode = (typeof ROUTES)[keyof typeof ROUTES];
