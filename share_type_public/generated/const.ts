/* Generated from src/const.rs. DO NOT EDIT. */

export const ROUTES = {
  CREATE: 1,
  JOIN: 2,
  QUIT: 3,
  MESSAGE: 4,
  PAUSE: 5,
  RESUME: 6,
  DISBAND: 7,
  SETTING: 8,
  DEAL: 20,
  PLAY: 21,
  AWAY: 22,
} as const;

export type RouteCode = (typeof ROUTES)[keyof typeof ROUTES];

export const CODE = {
  JOIN: 2,
  QUIT: 3,
  MESSAGE: 4,
  PAUSE: 5,
  RESUME: 6,
  DISBAND: 7,
  SETTING: 8,
  DEAL: 20,
  PLAY: 21,
  AWAY: 22,
} as const;

export type WsCode = (typeof CODE)[keyof typeof CODE];
