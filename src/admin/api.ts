export type AdminSession = {
  user: { id: string; email: string; name: string; role: string };
  csrf: string;
};
export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
  ) {
    super(code);
  }
}
export async function request<T>(
  path: string,
  method = "GET",
  body?: unknown,
  csrf?: string,
): Promise<T> {
  const response = await fetch("/v1" + path, {
    method,
    credentials: "same-origin",
    headers: {
      Accept: "application/json",
      ...(body !== undefined ? { "Content-Type": "application/json" } : {}),
      ...(csrf ? { "X-CSRF-Token": csrf } : {}),
    },
    ...(body !== undefined ? { body: JSON.stringify(body) } : {}),
  });
  const result = await response.json().catch(() => ({}));
  if (!response.ok)
    throw new ApiError(response.status, result.error || "request_failed");
  return result as T;
}
export function errorMessage(error: unknown) {
  if (!(error instanceof ApiError)) return "无法连接服务器，请检查网络后重试。";
  if (error.status === 429) return "请求过于频繁，请稍后重试。";
  if (error.status === 401 || error.status === 403)
    return "身份验证失败或会话已过期。请确认管理员账号、凭据及两步验证码。";
  const messages: Record<string, string> = {
    invalid_announcement: "请填写公告标题和内容，标题最多 160 字，内容最多 2000 字。",
    announcement_limit: "公告列表已满，请先移除不再需要的公告。",
    last_admin: "必须保留至少一位有效管理员。",
    self_change: "不能停用或降级当前管理员。",
    invalid_catalog:
      "目录内容无效，请检查名称、唯一 ID、网站 HTTPS 地址和图标。",
    invalid_url: "请使用有效的公开 HTTPS 地址。",
    invalid_image: "请选择有效的 PNG、JPEG 或 WebP 图片。",
    email_not_configured: "服务器尚未配置邮件服务。",
    registration_disabled: "注册已关闭。",
  };
  if (messages[error.code]) return messages[error.code];
  if (error.status === 409)
    return "数据已被其他管理员修改。请刷新最新数据，重新确认后保存。";
  return `操作未完成（${error.code}），请检查输入后重试。`;
}
