export type Site = {
  id: string;
  name: string;
  url: string;
  icon: string;
  order: number;
  enabled: boolean;
};
export type Group = {
  id: string;
  name: string;
  nameEn: string;
  icon: string;
  order: number;
  enabled: boolean;
  sites: Site[];
};
export type Catalog = {
  revision: number;
  publishedRevision: number;
  groups: Group[];
};
export type Summary = {
  users: number;
  devices: number;
  activeSessions: number;
  cloudObjects: number;
  cloudBytes: number;
  publishedRevision: number;
  auditEvents: number;
};
export type SystemConfig = {
  smtpConfigured: boolean;
  googleConfigured: boolean;
  databaseReady: boolean;
  publicURL: string;
  registrationEnabled: boolean;
  supportURL: string;
};
export type Announcement = {id:string;title:string;body:string;revision:number;published:boolean;createdAt:string;updatedAt:string;publishedAt:string};
export type User = {
  id: string;
  email: string;
  name: string;
  role: string;
  disabled: boolean;
  createdAt: string;
  googleLinked: boolean;
  hasPassword: boolean;
  twoFactorEnabled: boolean;
  devices: number;
  cloudBytes: number;
};
export type Device = {
  id: string;
  accountId: string;
  email: string;
  name: string;
  status: string;
  epoch: number;
  createdAt: string;
  activeSessions: number;
};
export type CloudObject = {
  accountId: string;
  email: string;
  id: string;
  kind: string;
  revision: number;
  epoch: number;
  deleted: boolean;
  bytes: number;
  updatedAt: string;
  writer: string;
};
export type AuditEvent = {
  id: string;
  actorEmail: string;
  event: string;
  targetId: string;
  detail: unknown;
  createdAt: string;
};
export type List<T> = {
  items: T[];
  total: number;
  page: number;
  pageSize: number;
};
