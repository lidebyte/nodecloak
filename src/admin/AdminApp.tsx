import {
  useEffect,
  useId,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react";
import {
  ArrowDown,
  ArrowUp,
  ArrowUpRight,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Cloud,
  Database,
  FileText,
  Globe,
  LayoutDashboard,
  LoaderCircle,
  LogOut,
  Mail,
  Megaphone,
  Menu,
  Monitor,
  Pencil,
  Plus,
  RefreshCw,
  Save,
  Search,
  Send,
  Settings,
  ShieldCheck,
  Trash2,
  Upload,
  Users,
  X,
} from "lucide-react";
import mark from "./assets/mark.svg";
import MarkdownEditor from './MarkdownEditor';
import {markdownSummary} from '../MarkdownContent';
import { ApiError, errorMessage, request, type AdminSession } from "./api";
import type {
  AuditEvent,
  Catalog,
  CloudObject,
  Device,
  Group,
  List,
  Site,
  Summary,
  SystemConfig,
  User,
  Announcement,
} from "./types";

type Page =
  "overview" | "catalog" | "announcements" | "users" | "devices" | "cloud" | "audit" | "system";
type Api = <T>(path: string, method?: string, body?: unknown) => Promise<T>;
const pages = [
  { id: "overview", label: "概览", icon: LayoutDashboard },
  { id: "catalog", label: "网站目录", icon: Globe },
  { id: "announcements", label: "公告", icon: Megaphone },
  { id: "users", label: "用户", icon: Users },
  { id: "devices", label: "设备", icon: Monitor },
  { id: "cloud", label: "云端存储", icon: Cloud },
  { id: "audit", label: "操作日志", icon: FileText },
  { id: "system", label: "系统设置", icon: Settings },
] as const;
const builtinIcons = [
  "ai",
  "shopping",
  "claude",
  "chatgpt",
  "gemini",
  "grok",
  "muse",
  "amazon",
  "ebay",
  "etsy",
  "walmart",
  "shopee",
  "lazada",
  "globe",
];
const date = (value: string) =>
  value ? new Date(value).toLocaleString("zh-CN", { hour12: false }) : "—";
const bytes = (value: number) =>
  value >= 1073741824
    ? (value / 1073741824).toFixed(1) + " GB"
    : value >= 1048576
      ? (value / 1048576).toFixed(1) + " MB"
      : value >= 1024
        ? (value / 1024).toFixed(1) + " KB"
        : value + " B";
function Button({
  children,
  icon,
  primary,
  danger,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & {
  icon?: ReactNode;
  primary?: boolean;
  danger?: boolean;
}) {
  return (
    <button
      {...props}
      className={`btn ${primary ? "primary" : ""} ${danger ? "danger" : ""} ${props.className || ""}`}
    >
      {icon}
      {children}
    </button>
  );
}
function Status({ good, children }: { good: boolean; children: ReactNode }) {
  return (
    <span className={`status ${good ? "good" : ""}`}>
      <i />
      {children}
    </span>
  );
}
function Message({
  children,
  error = false,
}: {
  children: ReactNode;
  error?: boolean;
}) {
  return children ? (
    <div
      className={`message ${error ? "error" : ""}`}
      role={error ? "alert" : "status"}
    >
      {children}
    </div>
  ) : null;
}
function Loading() {
  return (
    <div className="empty">
      <LoaderCircle className="spin" size={24} />
      <span>正在加载…</span>
    </div>
  );
}
function Empty({ children = "暂无记录" }: { children?: ReactNode }) {
  return (
    <div className="empty">
      <Database size={28} />
      <span>{children}</span>
    </div>
  );
}
function Toggle({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      className={`switch ${checked ? "on" : ""}`}
      onClick={() => onChange(!checked)}
    >
      <span />
    </button>
  );
}
function Modal({
  title,
  children,
  onClose,
  className = "",
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  className?: string;
}) {
  const modalRef = useRef<HTMLElement>(null),
    closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    const previousFocus = document.activeElement as HTMLElement | null;
    const modal = modalRef.current!;
    const selector =
      'button:not(:disabled), input:not(:disabled), textarea:not(:disabled), [tabindex="0"]';
    const close = (e: KeyboardEvent) => {
      if ([...document.querySelectorAll('[role="dialog"]')].at(-1) !== modal)
        return;
      if (e.key === "Escape") {
        e.preventDefault();
        closeRef.current();
      }
      if (e.key === "Tab") {
        const targets = [
          ...modal.querySelectorAll<HTMLElement>(selector),
        ].filter((el) => el.offsetParent !== null);
        const first = targets[0],
          last = targets.at(-1);
        if (
          e.shiftKey &&
          (document.activeElement === first ||
            !modal.contains(document.activeElement))
        ) {
          e.preventDefault();
          last?.focus();
        } else if (
          !e.shiftKey &&
          (document.activeElement === last ||
            !modal.contains(document.activeElement))
        ) {
          e.preventDefault();
          first?.focus();
        }
      }
    };
    document.addEventListener("keydown", close);
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    modal.querySelector<HTMLElement>("input, textarea, button")?.focus();
    return () => {
      document.removeEventListener("keydown", close);
      document.body.style.overflow = previous;
      previousFocus?.focus();
    };
  }, []);
  const id = useId();
  return (
    <div
      className="backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <section
        ref={modalRef}
        className={`modal ${className}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby={id}
      >
        <header>
          <h2 id={id}>{title}</h2>
          <button className="icon-btn" aria-label="关闭" onClick={onClose}>
            <X size={18} />
          </button>
        </header>
        {children}
      </section>
    </div>
  );
}
function Select({
  value,
  choices,
  onChange,
  label,
}: {
  value: string;
  choices: { value: string; label: string }[];
  onChange: (value: string) => void;
  label: string;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="select">
      <button
        className="select-trigger"
        type="button"
        aria-label={label}
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        onKeyDown={(e) => {
          if (e.key === "Escape") setOpen(false);
        }}
      >
        {choices.find((c) => c.value === value)?.label || value}
        <ChevronDown size={16} />
      </button>
      {open && (
        <>
          <button
            className="select-scrim"
            aria-label="关闭选项"
            type="button"
            onClick={() => setOpen(false)}
          />
          <div className="select-menu" role="listbox" aria-label={label}>
            {choices.map((c) => (
              <button
                type="button"
                role="option"
                aria-selected={c.value === value}
                key={c.value}
                onClick={() => {
                  onChange(c.value);
                  setOpen(false);
                }}
              >
                {c.label}
                {c.value === value && <Check size={16} />}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
function Brand() {
  return (
    <div className="brand">
      <img src={mark} alt="" />
      <div>
        Node<strong>Cloak</strong>
        <small>ADMIN CONSOLE</small>
      </div>
    </div>
  );
}

export function AdminApp() {
  const [session, setSession] = useState<AdminSession | null>(null);
  const [checking, setChecking] = useState(true);
  const [page, setPage] = useState<Page>("overview");
  const [menu, setMenu] = useState(false);
  const [sessionError, setSessionError] = useState("");
  const [catalogDirty, setCatalogDirty] = useState(false),
    [pendingAction, setPendingAction] = useState<(() => void) | null>(null);
  const navigate = (target: Page) => {
    if (target === page) {
      setMenu(false);
      return;
    }
    const action = () => {
      setCatalogDirty(false);
      setPage(target);
      setMenu(false);
    };
    if (catalogDirty) setPendingAction(() => action);
    else action();
  };
  useEffect(() => {
    request<AdminSession>("/admin/session")
      .then(setSession)
      .catch((error) => {
        if (!(error instanceof ApiError && error.status === 401))
          setSessionError(errorMessage(error));
      })
      .finally(() => setChecking(false));
  }, []);
  const api: Api = async <T,>(path: string, method = "GET", body?: unknown) => {
    try {
      return await request<T>(path, method, body, session?.csrf);
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) {
        setSession(null);
        setSessionError("会话已过期，请重新登录。");
      }
      throw error;
    }
  };
  const logout = async () => {
    if (catalogDirty) {
      setPendingAction(() => () => {
        setCatalogDirty(false);
        void forceLogout();
      });
      return;
    }
    await forceLogout();
  };
  const forceLogout = async () => {
    try {
      await api("/admin/logout", "POST", {});
      setSession(null);
    } catch (error) {
      setSessionError(errorMessage(error));
    }
  };
  if (checking)
    return (
      <div className="login-shell">
        <Brand />
        <Loading />
      </div>
    );
  if (!session)
    return (
      <Login
        onLogin={(s) => {
          setSession(s);
          setSessionError("");
        }}
        initialError={sessionError}
      />
    );
  return (
    <div className="admin-shell">
      <aside className={menu ? "expanded" : ""}>
        <Brand />
        <span className="eyebrow">工作空间</span>
        <nav>
          {pages.map((p) => (
            <button
              key={p.id}
              className={page === p.id ? "active" : ""}
              onClick={() => {
                navigate(p.id);
              }}
            >
              <p.icon size={18} />
              {p.label}
            </button>
          ))}
        </nav>
        <div className="sidebar-footer">
          <div className="avatar">
            {(session.user.name || session.user.email)
              .slice(0, 2)
              .toUpperCase()}
          </div>
          <div>
            <strong>{session.user.name || session.user.email}</strong>
            <small>管理员</small>
          </div>
          <button
            className="icon-btn"
            title="退出登录"
            aria-label="退出登录"
            onClick={logout}
          >
            <LogOut size={18} />
          </button>
        </div>
      </aside>
      <main>
        <header className="topbar">
          <button
            className="icon-btn mobile-nav"
            aria-label="展开导航"
            onClick={() => setMenu(!menu)}
          >
            <Menu size={20} />
          </button>
          <div>
            NodeCloak <ChevronRight size={15} />
            <span>{pages.find((p) => p.id === page)?.label}</span>
          </div>
          <span className="admin-badge">
            <ShieldCheck size={15} />
            管理员
          </span>
        </header>
        <div className="content">
          <div className="page-title">
            <span className="eyebrow">NODECLOAK / ADMINISTRATION</span>
            <h1>{pages.find((p) => p.id === page)?.label}</h1>
          </div>
          {sessionError && <Message error>{sessionError}</Message>}
          {page === "overview" ? (
            <Overview api={api} navigate={navigate} />
          ) : page === "catalog" ? (
            <CatalogPage api={api} onDirty={setCatalogDirty} />
          ) : page === "announcements" ? (
            <AnnouncementsPanel api={api} />
          ) : page === "system" ? (
            <SystemPage api={api} />
          ) : (
            <DataPage
              key={page}
              page={page}
              api={api}
              currentId={session.user.id}
            />
          )}
        </div>
      </main>
      {pendingAction && (
        <Confirm
          title="放弃未保存的修改"
          text="网站目录还有未保存的修改。继续后将丢弃这些修改。"
          onClose={() => setPendingAction(null)}
          onConfirm={() => {
            pendingAction();
            setPendingAction(null);
          }}
        />
      )}
    </div>
  );
}
function Login({
  onLogin,
  initialError,
}: {
  onLogin: (s: AdminSession) => void;
  initialError: string;
}) {
  const [mode, setMode] = useState("password"),
    [email, setEmail] = useState(""),
    [password, setPassword] = useState(""),
    [code, setCode] = useState(""),
    [factor, setFactor] = useState("");
  const [busy, setBusy] = useState(false),
    [message, setMessage] = useState(""),
    [error, setError] = useState(initialError),
    [cooldown, setCooldown] = useState(0);
  useEffect(() => {
    if (!cooldown) return;
    const timer = setTimeout(() => setCooldown(cooldown - 1), 1000);
    return () => clearTimeout(timer);
  }, [cooldown]);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      onLogin(
        await request<AdminSession>("/admin/login", "POST", {
          email,
          ...(mode === "password" ? { password } : { emailCode: code }),
          ...(factor.trim() ? { factor: factor.trim() } : {}),
        }),
      );
      setPassword("");
      setCode("");
      setFactor("");
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };
  const sendCode = async () => {
    setBusy(true);
    setError("");
    try {
      await request("/admin/email/request", "POST", { email });
      setMessage("如该邮箱拥有管理员权限，验证码将发送至邮箱。");
      setCooldown(60);
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="login-shell">
      <div className="login-card">
        <Brand />
        <div className="login-heading">
          <span className="eyebrow">ADMINISTRATION</span>
          <h1>管理后台</h1>
          <p>使用已授权的管理员账号登录。</p>
        </div>
        <div className="tabs">
          <button
            className={mode === "password" ? "selected" : ""}
            onClick={() => setMode("password")}
          >
            密码登录
          </button>
          <button
            className={mode === "email" ? "selected" : ""}
            onClick={() => setMode("email")}
          >
            邮箱验证码
          </button>
        </div>
        <form onSubmit={submit}>
          <label>
            邮箱
            <input
              type="email"
              autoComplete="username"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
            />
          </label>
          {mode === "password" ? (
            <label>
              密码
              <input
                type="password"
                autoComplete="current-password"
                required
                value={password}
                onChange={(e) => setPassword(e.target.value)}
              />
            </label>
          ) : (
            <label>
              邮箱验证码
              <div className="inline-field">
                <input
                  required
                  autoComplete="one-time-code"
                  value={code}
                  onChange={(e) => setCode(e.target.value)}
                />
                <Button
                  type="button"
                  icon={<Mail size={16} />}
                  disabled={busy || !email || cooldown > 0}
                  onClick={sendCode}
                >
                  {cooldown ? `${cooldown}s` : "发送验证码"}
                </Button>
              </div>
            </label>
          )}
          <label>
            两步验证码或恢复码
            <input
              autoComplete="off"
              value={factor}
              onChange={(e) => setFactor(e.target.value)}
              placeholder="账号已开启 2FA 时填写"
            />
          </label>
          <Message error>{error}</Message>
          <Message>{message}</Message>
          <Button
            type="submit"
            primary
            disabled={busy}
            icon={
              busy ? (
                <LoaderCircle className="spin" size={17} />
              ) : (
                <ShieldCheck size={17} />
              )
            }
          >
            登录
          </Button>
        </form>
      </div>
      <span className="login-foot">NodeCloak · Browser Workspace</span>
    </div>
  );
}
function Overview({
  api,
  navigate,
}: {
  api: Api;
  navigate: (p: Page) => void;
}) {
  const [summary, setSummary] = useState<Summary>(),
    [system, setSystem] = useState<SystemConfig>(),
    [error, setError] = useState(""),
    [loading, setLoading] = useState(true);
  const load = async () => {
    setLoading(true);
    setError("");
    const results = await Promise.allSettled([
      api<Summary>("/admin/summary"),
      api<SystemConfig>("/admin/system"),
    ]);
    if (results[0].status === "fulfilled") setSummary(results[0].value);
    if (results[1].status === "fulfilled") setSystem(results[1].value);
    const failed = results.find((r) => r.status === "rejected");
    if (failed?.status === "rejected") setError(errorMessage(failed.reason));
    setLoading(false);
  };
  useEffect(() => {
    void load();
  }, []);
  return (
    <>
      <div className="section-toolbar">
        <p>账号、设备与云端服务的当前状态。</p>
        <Button
          icon={<RefreshCw size={16} />}
          disabled={loading}
          onClick={load}
        >
          刷新
        </Button>
      </div>
      <Message error>{error}</Message>
      {loading ? (
        <Loading />
      ) : (
        <>
          {summary && (
            <div className="metrics">
              {(
                [
                  {
                    label: "用户",
                    value: summary.users,
                    icon: Users,
                    target: "users",
                  },
                  {
                    label: "设备",
                    value: summary.devices,
                    icon: Monitor,
                    target: "devices",
                  },
                  {
                    label: "活跃会话",
                    value: summary.activeSessions,
                    icon: ShieldCheck,
                    target: "devices",
                  },
                  {
                    label: "云端对象",
                    value: summary.cloudObjects,
                    icon: Cloud,
                    target: "cloud",
                  },
                  {
                    label: "云端存储",
                    value: bytes(summary.cloudBytes),
                    icon: Database,
                    target: "cloud",
                  },
                  {
                    label: "操作日志",
                    value: summary.auditEvents,
                    icon: FileText,
                    target: "audit",
                  },
                ] as const
              ).map((m) => (
                <button
                  className="metric"
                  key={m.label}
                  onClick={() => navigate(m.target)}
                >
                  <div>
                    <m.icon size={20} />
                    <ArrowUpRight size={16} />
                  </div>
                  <strong>{m.value}</strong>
                  <span>{m.label}</span>
                </button>
              ))}
            </div>
          )}
          <div className="overview-grid">
            <section className="panel">
              <div className="panel-heading">
                <h2>服务状态</h2>
                <Settings size={18} />
              </div>
              {system ? (
                <>
                  {[
                    ["数据库", system.databaseReady],
                    ["邮件服务", system.smtpConfigured],
                    ["Google 登录", system.googleConfigured],
                  ].map(([label, ready]) => (
                    <div className="setting-row" key={String(label)}>
                      <span>{label}</span>
                      <Status good={Boolean(ready)}>
                        {ready ? "已就绪" : "未配置"}
                      </Status>
                    </div>
                  ))}
                  <div className="setting-row">
                    <span>用户注册</span>
                    <Status good={system.registrationEnabled}>
                      {system.registrationEnabled ? "开放" : "关闭"}
                    </Status>
                  </div>
                </>
              ) : (
                <Empty />
              )}
            </section>
            <section className="panel publish-card">
              <span className="eyebrow">WEBSITE CATALOG</span>
              <h2>网站目录</h2>
              <strong className="revision">
                {summary?.publishedRevision ?? "—"}
                <small>已发布版本</small>
              </strong>
              <p>编辑分组、网站与品牌图标，保存草稿后发布到客户端。</p>
              <Button
                primary
                icon={<Globe size={16} />}
                onClick={() => navigate("catalog")}
              >
                管理目录
              </Button>
            </section>
          </div>
        </>
      )}
    </>
  );
}
function IconImage({ icon }: { icon: string }) {
  return (
    <img
      className="site-icon"
      src={
        icon.startsWith("/v1/catalog/icons/")
          ? icon
          : `/v1/catalog/icons/${icon}`
      }
      alt=""
      onError={(e) => {
        e.currentTarget.style.visibility = "hidden";
      }}
    />
  );
}
function IconPicker({
  value,
  onChange,
  api,
}: {
  value: string;
  onChange: (icon: string) => void;
  api: Api;
}) {
  const [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const upload = async (file?: File) => {
    if (!file) return;
    if (
      file.size > 2 * 1024 * 1024 ||
      !["image/png", "image/jpeg", "image/webp"].includes(file.type)
    ) {
      setError("请选择 2 MB 以内的 PNG、JPEG 或 WebP 图片。");
      return;
    }
    setBusy(true);
    setError("");
    try {
      const data = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.onerror = reject;
        reader.readAsDataURL(file);
      });
      const result = await api<{ icon: string }>("/admin/icons", "POST", {
        data,
      });
      if (alive.current) onChange(result.icon);
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="icon-picker">
      <span className="field-label">图标</span>
      <div className="icon-grid">
        {builtinIcons.map((icon) => (
          <button
            type="button"
            key={icon}
            aria-label={icon}
            title={icon}
            aria-pressed={icon === value}
            className={icon === value ? "selected" : ""}
            onClick={() => onChange(icon)}
          >
            <IconImage icon={icon} />
          </button>
        ))}
        {value.startsWith("/v1/") && (
          <button type="button" className="selected" aria-label="已上传图标">
            <IconImage icon={value} />
          </button>
        )}
      </div>
      <label className={`btn upload-button ${busy ? "disabled" : ""}`}>
        <Upload size={16} />
        {busy ? "正在上传…" : "上传图标"}
        <input
          type="file"
          accept="image/png,image/jpeg,image/webp"
          disabled={busy}
          onChange={(e) => void upload(e.target.files?.[0])}
        />
      </label>
      <small>PNG、JPEG 或 WebP，最大 2 MB。上传后保存草稿并发布。</small>
      <Message error>{error}</Message>
    </div>
  );
}
function CatalogPage({
  api,
  onDirty,
}: {
  api: Api;
  onDirty: (value: boolean) => void;
}) {
  const [catalog, setCatalog] = useState<Catalog>(),
    [loading, setLoading] = useState(true),
    [busy, setBusy] = useState(false),
    [dirty, setDirty] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const [groupEditor, setGroupEditor] = useState<{
      value: Group;
      isNew: boolean;
    }>(),
    [siteEditor, setSiteEditor] = useState<{
      groupId: string;
      value: Site;
      isNew: boolean;
    }>(),
    [confirm, setConfirm] = useState<{
      title: string;
      text: string;
      action: () => void;
    }>();
  const load = async () => {
    setLoading(true);
    setError("");
    try {
      setCatalog(await api<Catalog>("/admin/catalog"));
      setDirty(false);
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => {
    void load();
  }, []);
  useEffect(() => {
    onDirty(dirty);
  }, [dirty, onDirty]);
  useEffect(() => {
    if (!dirty) return;
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault();
    };
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, [dirty]);
  const update = (groups: Group[]) => {
    if (catalog) {
      setCatalog({ ...catalog, groups });
      setDirty(true);
      setNotice("");
    }
  };
  const persist = async (publish: boolean) => {
    if (!catalog) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = publish
        ? await api<Catalog>("/admin/catalog/publish", "POST", {
            revision: catalog.revision,
          })
        : await api<Catalog>("/admin/catalog", "PUT", {
            revision: catalog.revision,
            groups: catalog.groups,
          });
      setCatalog(result);
      setDirty(false);
      setNotice(
        publish
          ? "目录已发布，客户端将加载新的分组、网站和图标。"
          : "草稿已保存，发布后才会更新客户端。",
      );
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };
  const move = (
    groupId: string,
    siteId: string | undefined,
    direction: number,
  ) => {
    if (!catalog) return;
    const ordered = [...catalog.groups].sort((a, b) => a.order - b.order);
    const group = ordered.find((g) => g.id === groupId)!;
    const list = siteId
      ? [...group.sites].sort((a, b) => a.order - b.order)
      : ordered;
    const index = list.findIndex((item) => item.id === (siteId || groupId)),
      next = index + direction;
    if (next < 0 || next >= list.length) return;
    [list[index], list[next]] = [list[next], list[index]];
    if (siteId)
      update(
        ordered.map((g) =>
          g.id === groupId
            ? {
                ...g,
                sites: (list as Site[]).map((s, i) => ({ ...s, order: i })),
              }
            : g,
        ),
      );
    else update((list as Group[]).map((g, i) => ({ ...g, order: i })));
  };
  const newId = (prefix: string) =>
    prefix + "_" + crypto.randomUUID().replaceAll("-", "").slice(0, 12);
  const closeGroup = () => {
    if (!groupEditor) return;
    const changed = groupEditor.isNew
      ? Boolean(
          groupEditor.value.name ||
          groupEditor.value.nameEn ||
          groupEditor.value.icon !== "globe" ||
          !groupEditor.value.enabled,
        )
      : JSON.stringify(groupEditor.value) !==
        JSON.stringify(
          catalog?.groups.find((g) => g.id === groupEditor.value.id),
        );
    if (changed)
      setConfirm({
        title: "放弃分组修改",
        text: "尚未确认的分组修改将被丢弃。",
        action: () => setGroupEditor(undefined),
      });
    else setGroupEditor(undefined);
  };
  const closeSite = () => {
    if (!siteEditor) return;
    const changed = siteEditor.isNew
      ? Boolean(
          siteEditor.value.name ||
          siteEditor.value.url !== "https://" ||
          siteEditor.value.icon !== "globe" ||
          !siteEditor.value.enabled,
        )
      : JSON.stringify(siteEditor.value) !==
        JSON.stringify(
          catalog?.groups
            .find((g) => g.id === siteEditor.groupId)
            ?.sites.find((s) => s.id === siteEditor.value.id),
        );
    if (changed)
      setConfirm({
        title: "放弃网站修改",
        text: "尚未确认的网站修改将被丢弃。",
        action: () => setSiteEditor(undefined),
      });
    else setSiteEditor(undefined);
  };
  return (
    <>
      <div className="section-toolbar">
        <div>
          <p>统一管理客户端的网站入口。</p>
          <div className="catalog-meta">
            <span>草稿 v{catalog?.revision ?? "—"}</span>
            <span>已发布 v{catalog?.publishedRevision ?? "—"}</span>
            {dirty && <span className="dirty">尚未保存</span>}
          </div>
        </div>
        <div className="actions">
          <Button
            disabled={loading || busy}
            icon={<RefreshCw size={16} />}
            onClick={() =>
              dirty
                ? setConfirm({
                    title: "重新加载草稿",
                    text: "未保存的修改将被丢弃。",
                    action: () => void load(),
                  })
                : void load()
            }
          >
            刷新
          </Button>
          <Button
            icon={<Save size={16} />}
            disabled={!dirty || busy}
            onClick={() => void persist(false)}
          >
            保存草稿
          </Button>
          <Button
            primary
            icon={<Send size={16} />}
            disabled={
              !catalog ||
              dirty ||
              busy ||
              catalog.revision === catalog.publishedRevision
            }
            onClick={() =>
              setConfirm({
                title: "发布网站目录",
                text: "发布当前草稿，客户端将使用此版本。",
                action: () => void persist(true),
              })
            }
          >
            发布
          </Button>
        </div>
      </div>
      <Message error>{error}</Message>
      <Message>{notice}</Message>
      {loading ? (
        <Loading />
      ) : (
        catalog && (
          <>
            <div className="catalog-toolbar">
              <h2>
                分组 <span className="count">{catalog.groups.length}</span>
              </h2>
              <Button
                primary
                disabled={busy}
                icon={<Plus size={16} />}
                onClick={() =>
                  setGroupEditor({
                    isNew: true,
                    value: {
                      id: newId("group"),
                      name: "",
                      nameEn: "",
                      icon: "globe",
                      enabled: true,
                      order: catalog.groups.length,
                      sites: [],
                    },
                  })
                }
              >
                添加分组
              </Button>
            </div>
            {catalog.groups.length ? (
              [...catalog.groups]
                .sort((a, b) => a.order - b.order)
                .map((group, index, groups) => (
                  <section className="panel group-panel" key={group.id}>
                    <div className="group-heading">
                      <IconImage icon={group.icon} />
                      <div>
                        <h2>
                          {group.name} <small>{group.nameEn}</small>
                        </h2>
                        <span className="mono">{group.id}</span>
                      </div>
                      <Status good={group.enabled}>
                        {group.enabled ? "启用" : "停用"}
                      </Status>
                      <div className="actions">
                        <button
                          className="icon-btn"
                          title="上移分组"
                          aria-label={`上移 ${group.name}`}
                          disabled={index === 0 || busy}
                          onClick={() => move(group.id, undefined, -1)}
                        >
                          <ArrowUp size={16} />
                        </button>
                        <button
                          className="icon-btn"
                          title="下移分组"
                          aria-label={`下移 ${group.name}`}
                          disabled={index === groups.length - 1 || busy}
                          onClick={() => move(group.id, undefined, 1)}
                        >
                          <ArrowDown size={16} />
                        </button>
                        <button
                          className="icon-btn"
                          title="编辑分组"
                          aria-label={`编辑 ${group.name}`}
                          onClick={() =>
                            setGroupEditor({
                              isNew: false,
                              value: structuredClone(group),
                            })
                          }
                        >
                          <Pencil size={16} />
                        </button>
                        <button
                          className="icon-btn danger"
                          title="删除分组"
                          aria-label={`删除 ${group.name}`}
                          onClick={() =>
                            setConfirm({
                              title: "删除分组",
                              text: `将从草稿删除「${group.name}」及其 ${group.sites.length} 个网站。发布后生效。`,
                              action: () =>
                                update(
                                  catalog.groups.filter(
                                    (g) => g.id !== group.id,
                                  ),
                                ),
                            })
                          }
                        >
                          <Trash2 size={16} />
                        </button>
                      </div>
                    </div>
                    <div className="site-list">
                      {[...group.sites]
                        .sort((a, b) => a.order - b.order)
                        .map((site, i, sites) => (
                          <div className="site-row" key={site.id}>
                            <IconImage icon={site.icon} />
                            <div>
                              <strong>{site.name}</strong>
                              <a
                                href={site.url}
                                target="_blank"
                                rel="noopener noreferrer"
                              >
                                {site.url}
                                <ArrowUpRight size={12} />
                              </a>
                            </div>
                            <Status good={site.enabled}>
                              {site.enabled ? "启用" : "停用"}
                            </Status>
                            <div className="actions">
                              <button
                                className="icon-btn"
                                aria-label={`上移 ${site.name}`}
                                disabled={i === 0}
                                onClick={() => move(group.id, site.id, -1)}
                              >
                                <ArrowUp size={15} />
                              </button>
                              <button
                                className="icon-btn"
                                aria-label={`下移 ${site.name}`}
                                disabled={i === sites.length - 1}
                                onClick={() => move(group.id, site.id, 1)}
                              >
                                <ArrowDown size={15} />
                              </button>
                              <button
                                className="icon-btn"
                                aria-label={`编辑 ${site.name}`}
                                onClick={() =>
                                  setSiteEditor({
                                    groupId: group.id,
                                    value: { ...site },
                                    isNew: false,
                                  })
                                }
                              >
                                <Pencil size={15} />
                              </button>
                              <button
                                className="icon-btn danger"
                                aria-label={`删除 ${site.name}`}
                                onClick={() =>
                                  setConfirm({
                                    title: "删除网站",
                                    text: `将从草稿删除「${site.name}」。发布后生效。`,
                                    action: () =>
                                      update(
                                        catalog.groups.map((g) =>
                                          g.id === group.id
                                            ? {
                                                ...g,
                                                sites: g.sites.filter(
                                                  (s) => s.id !== site.id,
                                                ),
                                              }
                                            : g,
                                        ),
                                      ),
                                  })
                                }
                              >
                                <Trash2 size={15} />
                              </button>
                            </div>
                          </div>
                        ))}
                    </div>
                    <Button
                      icon={<Plus size={15} />}
                      onClick={() =>
                        setSiteEditor({
                          groupId: group.id,
                          isNew: true,
                          value: {
                            id: newId("site"),
                            name: "",
                            url: "https://",
                            icon: "globe",
                            order: group.sites.length,
                            enabled: true,
                          },
                        })
                      }
                    >
                      添加网站
                    </Button>
                  </section>
                ))
            ) : (
              <Empty>尚无分组，点击“添加分组”创建第一个入口。</Empty>
            )}
          </>
        )
      )}
      {groupEditor && (
        <Modal
          title={groupEditor.isNew ? "添加分组" : "编辑分组"}
          onClose={closeGroup}
        >
          <form
            onSubmit={(e) => {
              e.preventDefault();
              const value = groupEditor.value;
              update(
                groupEditor.isNew
                  ? [...catalog!.groups, value]
                  : catalog!.groups.map((g) => (g.id === value.id ? value : g)),
              );
              setGroupEditor(undefined);
            }}
          >
            <label>
              分组名称
              <input
                autoFocus
                required
                maxLength={80}
                value={groupEditor.value.name}
                onChange={(e) =>
                  setGroupEditor({
                    ...groupEditor,
                    value: { ...groupEditor.value, name: e.target.value },
                  })
                }
              />
            </label>
            <label>
              英文名称
              <input
                required
                maxLength={80}
                value={groupEditor.value.nameEn}
                onChange={(e) =>
                  setGroupEditor({
                    ...groupEditor,
                    value: { ...groupEditor.value, nameEn: e.target.value },
                  })
                }
              />
            </label>
            <IconPicker
              api={api}
              value={groupEditor.value.icon}
              onChange={(icon) =>
                setGroupEditor({
                  ...groupEditor,
                  value: { ...groupEditor.value, icon },
                })
              }
            />
            <div className="setting-row">
              <span>启用分组</span>
              <Toggle
                label="启用分组"
                checked={groupEditor.value.enabled}
                onChange={(enabled) =>
                  setGroupEditor({
                    ...groupEditor,
                    value: { ...groupEditor.value, enabled },
                  })
                }
              />
            </div>
            <div className="modal-actions">
              <Button type="button" onClick={closeGroup}>
                取消
              </Button>
              <Button type="submit" primary icon={<Check size={16} />}>
                确认
              </Button>
            </div>
          </form>
        </Modal>
      )}
      {siteEditor && (
        <Modal
          title={siteEditor.isNew ? "添加网站" : "编辑网站"}
          onClose={closeSite}
        >
          <form
            onSubmit={(e) => {
              e.preventDefault();
              const value = siteEditor.value;
              update(
                catalog!.groups.map((g) =>
                  g.id === siteEditor.groupId
                    ? {
                        ...g,
                        sites: siteEditor.isNew
                          ? [...g.sites, value]
                          : g.sites.map((s) => (s.id === value.id ? value : s)),
                      }
                    : g,
                ),
              );
              setSiteEditor(undefined);
            }}
          >
            <label>
              网站名称
              <input
                autoFocus
                required
                maxLength={80}
                value={siteEditor.value.name}
                onChange={(e) =>
                  setSiteEditor({
                    ...siteEditor,
                    value: { ...siteEditor.value, name: e.target.value },
                  })
                }
              />
            </label>
            <label>
              网站地址
              <input
                required
                type="url"
                pattern="https://.*"
                value={siteEditor.value.url}
                onChange={(e) =>
                  setSiteEditor({
                    ...siteEditor,
                    value: { ...siteEditor.value, url: e.target.value },
                  })
                }
              />
              <small>公开 HTTPS 地址，不含用户名、密码或非标准端口。</small>
            </label>
            <IconPicker
              api={api}
              value={siteEditor.value.icon}
              onChange={(icon) =>
                setSiteEditor({
                  ...siteEditor,
                  value: { ...siteEditor.value, icon },
                })
              }
            />
            <div className="setting-row">
              <span>启用网站</span>
              <Toggle
                label="启用网站"
                checked={siteEditor.value.enabled}
                onChange={(enabled) =>
                  setSiteEditor({
                    ...siteEditor,
                    value: { ...siteEditor.value, enabled },
                  })
                }
              />
            </div>
            <div className="modal-actions">
              <Button type="button" onClick={closeSite}>
                取消
              </Button>
              <Button type="submit" primary icon={<Check size={16} />}>
                确认
              </Button>
            </div>
          </form>
        </Modal>
      )}
      {confirm && (
        <Confirm
          title={confirm.title}
          text={confirm.text}
          onClose={() => setConfirm(undefined)}
          onConfirm={() => {
            confirm.action();
            setConfirm(undefined);
          }}
        />
      )}
    </>
  );
}
function Confirm({
  title,
  text,
  onClose,
  onConfirm,
  busy = false,
  error = "",
}: {
  title: string;
  text: string;
  onClose: () => void;
  onConfirm: () => void;
  busy?: boolean;
  error?: string;
}) {
  return (
    <Modal title={title} onClose={onClose}>
      <p className="confirmation-text">{text}</p>
      <Message error>{error}</Message>
      <div className="modal-actions">
        <Button disabled={busy} onClick={onClose}>
          取消
        </Button>
        <Button
          primary
          disabled={busy}
          icon={
            busy ? (
              <LoaderCircle className="spin" size={16} />
            ) : (
              <Check size={16} />
            )
          }
          onClick={onConfirm}
        >
          确认
        </Button>
      </div>
    </Modal>
  );
}
function SystemPage({ api }: { api: Api }) {
  const [config, setConfig] = useState<SystemConfig>(),
    [loading, setLoading] = useState(true),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [message, setMessage] = useState("");
  const load = async () => {
    setLoading(true);
    setError("");
    try {
      setConfig(await api<SystemConfig>("/admin/system"));
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => {
    void load();
  }, []);
  const save = async (e: FormEvent) => {
    e.preventDefault();
    if (!config) return;
    setBusy(true);
    setError("");
    setMessage("");
    try {
      setConfig(
        await api<SystemConfig>("/admin/system", "PUT", {
          registrationEnabled: config.registrationEnabled,
          supportURL: config.supportURL,
        }),
      );
      setMessage("系统设置已保存。");
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <div className="section-toolbar">
        <p>服务配置与客户端公共信息。</p>
        <Button
          disabled={loading || busy}
          icon={<RefreshCw size={16} />}
          onClick={load}
        >
          刷新
        </Button>
      </div>
      <Message error>{error}</Message>
      <Message>{message}</Message>
      {loading ? (
        <Loading />
      ) : (
        config && (
          <div className="system-grid">
            <section className="panel">
              <div className="panel-heading">
                <h2>服务配置</h2>
                <ShieldCheck size={18} />
              </div>
              {[
                ["数据库", config.databaseReady],
                ["SMTP 邮件", config.smtpConfigured],
                ["Google OAuth", config.googleConfigured],
              ].map(([name, ready]) => (
                <div className="setting-row" key={String(name)}>
                  <span>{name}</span>
                  <Status good={Boolean(ready)}>
                    {ready ? "已就绪" : "未配置"}
                  </Status>
                </div>
              ))}
              <label className="readonly-label">
                公共服务地址
                <span className="mono">{config.publicURL || "—"}</span>
              </label>
              <p className="muted note">
                OAuth、SMTP 和数据库凭据由服务器部署配置维护。
              </p>
            </section>
            <form className="panel" onSubmit={save}>
              <div className="panel-heading">
                <h2>公共设置</h2>
                <Settings size={18} />
              </div>
              <div className="setting-row">
                <div>
                  <strong>开放注册</strong>
                  <small>关闭后，已有账号仍可登录。</small>
                </div>
                <Toggle
                  label="开放注册"
                  checked={config.registrationEnabled}
                  disabled={busy}
                  onChange={(registrationEnabled) =>
                    setConfig({ ...config, registrationEnabled })
                  }
                />
              </div>
              <label>
                支持链接
                <input
                  type="url"
                  pattern="https://.*"
                  placeholder="https://"
                  value={config.supportURL}
                  onChange={(e) =>
                    setConfig({ ...config, supportURL: e.target.value })
                  }
                />
              </label>
              <div className="modal-actions">
                <Button primary disabled={busy} icon={<Save size={16} />}>
                  保存设置
                </Button>
              </div>
            </form>
          </div>
        )
      )}
    </>
  );
}
export function AnnouncementsPanel({api}:{api:Api}) {
 const pageSize=10;
 const [items,setItems]=useState<Announcement[]>([]),[total,setTotal]=useState(0),[page,setPage]=useState(1),[loading,setLoading]=useState(true),[busy,setBusy]=useState(false),[error,setError]=useState(''),[message,setMessage]=useState('');
 const requestNumber=useRef(0);
 const [editor,setEditor]=useState<Announcement>(),[dirty,setDirty]=useState(false),[discard,setDiscard]=useState(false),[remove,setRemove]=useState<Announcement>();
 const load=async(target=page)=>{
  const request=++requestNumber.current;setLoading(true);setError('');
  try{
   const data=await api<List<Announcement>>(`/admin/announcements?page=${target}&pageSize=${pageSize}`);
   if(request!==requestNumber.current)return;
   setTotal(data.total);
   const lastPage=Math.max(1,Math.ceil(data.total/pageSize));
   if(target>lastPage){setPage(lastPage);return;}
   setItems(data.items);
  }catch(e){if(request===requestNumber.current)setError(errorMessage(e));}
  finally{if(request===requestNumber.current)setLoading(false);}
 };
 useEffect(()=>{void load(page);return()=>{requestNumber.current++;};},[page]);
 const write=async(item:Announcement)=>{
  setBusy(true);setError('');setMessage('');
  try{await api(`/admin/announcements${item.id?'/'+encodeURIComponent(item.id):''}`,item.id?'PATCH':'POST',{title:item.title,body:item.body,published:item.published,...(item.id?{revision:item.revision}:{})});setEditor(undefined);setDirty(false);setMessage('公告已保存。');if(!item.id&&page!==1)setPage(1);else await load();}catch(e){setError(errorMessage(e));}finally{setBusy(false);}
 };
 const archive=async()=>{
  if(!remove)return;setBusy(true);setError('');
  try{await api('/admin/announcements/'+encodeURIComponent(remove.id),'DELETE',{revision:remove.revision});setRemove(undefined);setMessage('公告已移除。');await load();}catch(e){setError(errorMessage(e));}finally{setBusy(false);}
 };
 const closeEditor=()=>{if(busy)return;if(dirty)setDiscard(true);else setEditor(undefined);};
 return <section className="panel announcements-panel">
  <div className="panel-heading"><div><h2>公告列表</h2><p className="muted note">客户端显示最近发布的一条，用户可关闭或查看历史公告。</p></div><div className="announcement-admin-actions"><Button disabled={busy||loading} icon={<RefreshCw size={16}/>} onClick={()=>void load()}>刷新</Button><Button primary disabled={busy||loading} icon={<Plus size={16}/>} onClick={()=>{setError('');setDirty(false);setEditor({id:'',title:'',body:'',published:true,revision:0,createdAt:'',updatedAt:'',publishedAt:''});}}>新增公告</Button></div></div>
  <Message error>{error}</Message><Message>{message}</Message>
  {loading?<Loading/>:items.length===0?<Empty>暂无公告</Empty>:<div className="announcement-admin-list">{items.map(item=><article key={item.id}><div className="announcement-admin-copy"><div><h3>{item.title}</h3><Status good={item.published}>{item.published?'已发布':'草稿'}</Status></div><p>{markdownSummary(item.body)}</p><small className="muted">{item.published?'发布时间':'更新时间'} · {date(item.published?item.publishedAt:item.updatedAt)}</small></div><div className="announcement-admin-actions"><Button disabled={busy} icon={<Pencil size={15}/>} onClick={()=>{setError('');setDirty(false);setEditor({...item});}}>编辑</Button><Button disabled={busy} icon={item.published?<X size={15}/>:<Send size={15}/>} onClick={()=>void write({...item,published:!item.published})}>{item.published?'撤下':'发布'}</Button><Button disabled={busy} danger icon={<Trash2 size={15}/>} onClick={()=>{setError('');setRemove(item);}}>移除</Button></div></article>)}</div>}
  <div className="pagination"><span>共 {total} 条公告 · 每页 {pageSize} 条</span><div><button className="icon-btn" aria-label="上一页" disabled={page<=1||loading||busy} onClick={()=>setPage(page-1)}><ChevronLeft size={16}/></button><span>第 {page} / {Math.max(1,Math.ceil(total/pageSize))} 页</span><button className="icon-btn" aria-label="下一页" disabled={page*pageSize>=total||loading||busy} onClick={()=>setPage(page+1)}><ChevronRight size={16}/></button></div></div>
  {editor&&<Modal className="announcement-editor-modal" title={editor.id?'编辑公告':'新增公告'} onClose={closeEditor}><form onSubmit={event=>{event.preventDefault();if(editor.title.trim()&&editor.body.trim())void write(editor);}}><label>标题<input autoFocus required disabled={busy} maxLength={160} value={editor.title} onChange={event=>{setDirty(true);setEditor({...editor,title:event.target.value});}}/></label><div className="announcement-content-label">内容</div><MarkdownEditor value={editor.body} disabled={busy} onChange={body=>{setDirty(true);setEditor({...editor,body});}}/><div className="setting-row"><div><strong>发布到客户端</strong><small>撤下的公告不会显示在客户端历史列表。</small></div><Toggle label="发布到客户端" checked={editor.published} disabled={busy} onChange={published=>{setDirty(true);setEditor({...editor,published});}}/></div><Message error>{error}</Message><div className="modal-actions"><Button type="button" disabled={busy} onClick={closeEditor}>取消</Button><Button primary disabled={busy||!editor.title.trim()||!editor.body.trim()} icon={<Save size={16}/>}>{editor.id?'保存变更':editor.published?'发布公告':'保存草稿'}</Button></div></form></Modal>}
  {discard&&<Confirm title="放弃编辑？" text="尚未保存的公告修改会丢失。" onClose={()=>setDiscard(false)} onConfirm={()=>{setDiscard(false);setEditor(undefined);setDirty(false);}}/>}
  {remove&&<Confirm title="移除公告？" text={`「${remove.title}」将从公告列表和客户端历史中移除。`} busy={busy} error={error} onClose={()=>{if(!busy)setRemove(undefined);}} onConfirm={()=>void archive()}/>}
 </section>;
}
type Row = User | Device | CloudObject | AuditEvent;
function DataPage({
  page,
  api,
  currentId,
}: {
  page: "users" | "devices" | "cloud" | "audit";
  api: Api;
  currentId: string;
}) {
  const [query, setQuery] = useState(""),
    [search, setSearch] = useState(""),
    [number, setNumber] = useState(1),
    [data, setData] = useState<List<Row>>(),
    [loading, setLoading] = useState(true),
    [error, setError] = useState(""),
    [message, setMessage] = useState(""),
    [busy, setBusy] = useState(false);
  const [edit, setEdit] = useState<User>(),
    [detail, setDetail] = useState<Row>(),
    [confirm, setConfirm] = useState<{
      title: string;
      text: string;
      path: string;
      body?: unknown;
      method?: string;
    }>();
  const load = async () => {
    setLoading(true);
    setError("");
    try {
      setData(
        await api<List<Row>>(
          `/admin/${page}?q=${encodeURIComponent(search)}&page=${number}&pageSize=20`,
        ),
      );
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => {
    void load();
  }, [page, search, number]);
  const mutate = async (path: string, body: unknown = {}, method = "POST") => {
    setBusy(true);
    setError("");
    setMessage("");
    try {
      await api(path, method, body);
      setConfirm(undefined);
      setEdit(undefined);
      setMessage("操作已完成。");
      await load();
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };
  const revokeUser = (user: User) =>
    setConfirm({
      title: "使登录会话失效",
      text: `「${user.email}」的登录会话将失效，需要重新登录。`,
      path: `/admin/users/${encodeURIComponent(user.id)}/sessions/revoke`,
    });
  const revokeDevice = (device: Device) =>
    setConfirm({
      title: "使设备登录会话失效",
      text: `「${device.name || device.id}」的登录会话将失效。此操作不会撤销设备的加密授权，也不会轮换云端密钥。`,
      path: `/admin/devices/${encodeURIComponent(device.accountId)}/${encodeURIComponent(device.id)}/sessions/revoke`,
    });
  const headers =
    page === "users"
      ? ["用户", "权限", "安全", "设备 / 存储", "状态", "操作"]
      : page === "devices"
        ? ["设备", "账号", "授权状态", "Epoch / 会话", "创建时间", "操作"]
        : page === "cloud"
          ? ["对象", "账号", "版本 / Epoch", "存储", "状态", "更新时间"]
          : ["事件", "操作者", "目标", "时间", "详情"];
  return (
    <>
      <div className="section-toolbar">
        <form
          className="search-form"
          onSubmit={(e) => {
            e.preventDefault();
            setNumber(1);
            setSearch(query);
          }}
        >
          <Search size={17} />
          <input
            aria-label="搜索记录"
            placeholder={
              page === "audit" ? "搜索事件、账号或目标" : "搜索账号或名称"
            }
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <Button type="submit">搜索</Button>
        </form>
        <Button
          disabled={loading || busy}
          icon={<RefreshCw size={16} />}
          onClick={load}
        >
          刷新
        </Button>
      </div>
      {page === "cloud" && (
        <p className="muted note">
          <ShieldCheck size={16} />
          仅展示对象元数据。云端内容保持加密。
        </p>
      )}
      <Message error>{error}</Message>
      <Message>{message}</Message>
      <section className="panel table-panel">
        {loading ? (
          <Loading />
        ) : !data?.items.length ? (
          <Empty>{search ? "未找到匹配的记录" : "暂无记录"}</Empty>
        ) : (
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  {headers.map((h) => (
                    <th key={h}>{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {data.items.map((row, index) => (
                  <tr
                    key={
                      "accountId" in row
                        ? `${row.accountId}:${row.id}:${"kind" in row ? row.kind : ""}`
                        : `${row.id}:${index}`
                    }
                  >
                    {page === "users"
                      ? (() => {
                          const u = row as User;
                          return (
                            <>
                              <td>
                                <strong>{u.name || "未设置名称"}</strong>
                                <small>{u.email}</small>
                                <span className="mono id">{u.id}</span>
                              </td>
                              <td>
                                <span
                                  className={`tag ${u.role === "admin" ? "green" : ""}`}
                                >
                                  {u.role === "admin" ? "管理员" : "用户"}
                                </span>
                              </td>
                              <td>
                                <div className="tags">
                                  {u.googleLinked && (
                                    <span className="tag">Google</span>
                                  )}
                                  {u.hasPassword && (
                                    <span className="tag">密码</span>
                                  )}
                                  {u.twoFactorEnabled && (
                                    <span className="tag green">2FA</span>
                                  )}
                                  {!u.googleLinked && !u.hasPassword && (
                                    <span className="tag">邮箱</span>
                                  )}
                                </div>
                              </td>
                              <td>
                                {u.devices} 台
                                <small>{bytes(u.cloudBytes)}</small>
                              </td>
                              <td>
                                <Status good={!u.disabled}>
                                  {u.disabled ? "已停用" : "正常"}
                                </Status>
                              </td>
                              <td>
                                <div className="actions">
                                  <button
                                    className="icon-btn"
                                    title="编辑用户"
                                    aria-label={`编辑 ${u.email}`}
                                    disabled={busy}
                                    onClick={() => setEdit({ ...u })}
                                  >
                                    <Pencil size={16} />
                                  </button>
                                  <button
                                    className="icon-btn danger"
                                    title="使登录会话失效"
                                    aria-label={`使 ${u.email} 会话失效`}
                                    disabled={busy}
                                    onClick={() => revokeUser(u)}
                                  >
                                    <LogOut size={16} />
                                  </button>
                                </div>
                              </td>
                            </>
                          );
                        })()
                      : page === "devices"
                        ? (() => {
                            const d = row as Device;
                            return (
                              <>
                                <td>
                                  <strong>{d.name || "未命名设备"}</strong>
                                  <span className="mono id">{d.id}</span>
                                </td>
                                <td>
                                  {d.email}
                                  <span className="mono id">{d.accountId}</span>
                                </td>
                                <td>
                                  <Status
                                    good={
                                      d.status === "approved" ||
                                      d.status === "authorized"
                                    }
                                  >
                                    {(
                                      {
                                        approved: "已授权",
                                        pending: "待授权",
                                        revoked: "已撤销",
                                        authorized: "已授权",
                                      } as Record<string, string>
                                    )[d.status] || d.status}
                                  </Status>
                                </td>
                                <td>
                                  {d.epoch}
                                  <small>{d.activeSessions} 个会话</small>
                                </td>
                                <td>{date(d.createdAt)}</td>
                                <td>
                                  <button
                                    className="icon-btn danger"
                                    title="使登录会话失效"
                                    aria-label={`使 ${d.name || d.id} 会话失效`}
                                    disabled={busy || d.activeSessions === 0}
                                    onClick={() => revokeDevice(d)}
                                  >
                                    <LogOut size={16} />
                                  </button>
                                </td>
                              </>
                            );
                          })()
                        : page === "cloud"
                          ? (() => {
                              const o = row as CloudObject;
                              return (
                                <>
                                  <td>
                                    <strong>{o.kind}</strong>
                                    <span className="mono id">{o.id}</span>
                                  </td>
                                  <td>
                                    {o.email}
                                    <small>写入设备：{o.writer || "—"}</small>
                                  </td>
                                  <td>
                                    {o.revision}
                                    <small>Epoch {o.epoch}</small>
                                  </td>
                                  <td>{bytes(o.bytes)}</td>
                                  <td>
                                    <Status good={!o.deleted}>
                                      {o.deleted ? "删除标记" : "有效"}
                                    </Status>
                                  </td>
                                  <td>{date(o.updatedAt)}</td>
                                </>
                              );
                            })()
                          : (() => {
                              const a = row as AuditEvent;
                              return (
                                <>
                                  <td>
                                    <strong className="mono">{a.event}</strong>
                                  </td>
                                  <td>{a.actorEmail || "系统"}</td>
                                  <td>
                                    <span className="mono id">
                                      {a.targetId || "—"}
                                    </span>
                                  </td>
                                  <td>{date(a.createdAt)}</td>
                                  <td>
                                    <button
                                      className="icon-btn"
                                      title="查看详情"
                                      aria-label="查看日志详情"
                                      onClick={() => setDetail(a)}
                                    >
                                      <FileText size={16} />
                                    </button>
                                  </td>
                                </>
                              );
                            })()}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        <div className="pagination">
          <span>共 {data?.total ?? 0} 条记录</span>
          <div>
            <button
              className="icon-btn"
              aria-label="上一页"
              disabled={number <= 1 || loading}
              onClick={() => setNumber(number - 1)}
            >
              <ChevronLeft size={16} />
            </button>
            <span>
              第 {number} / {Math.max(1, Math.ceil((data?.total ?? 0) / 20))} 页
            </span>
            <button
              className="icon-btn"
              aria-label="下一页"
              disabled={number * 20 >= (data?.total ?? 0) || loading}
              onClick={() => setNumber(number + 1)}
            >
              <ChevronRight size={16} />
            </button>
          </div>
        </div>
      </section>
      {edit && (
        <Modal
          title="编辑用户"
          onClose={() => {
            if (!busy) setEdit(undefined);
          }}
        >
          <form
            onSubmit={(e) => {
              e.preventDefault();
              setConfirm({
                title: "确认修改用户",
                text: `保存「${edit.email}」的名称、角色和账号状态。停用账号后将无法登录。`,
                method: "PATCH",
                path: `/admin/users/${encodeURIComponent(edit.id)}`,
                body: {
                  name: edit.name,
                  role: edit.role,
                  disabled: edit.disabled,
                },
              });
            }}
          >
            <p className="muted">{edit.email}</p>
            <label>
              显示名称
              <input
                required
                maxLength={80}
                value={edit.name}
                onChange={(e) => setEdit({ ...edit, name: e.target.value })}
              />
            </label>
            <label>
              账号权限
              <Select
                label="账号权限"
                value={edit.role}
                choices={[
                  { value: "user", label: "用户" },
                  { value: "admin", label: "管理员" },
                ]}
                onChange={(role) => {
                  if (edit.id !== currentId) setEdit({ ...edit, role });
                }}
              />
            </label>
            {edit.id === currentId && (
              <small className="muted">当前管理员不能被停用或降级。</small>
            )}
            <div className="setting-row">
              <span>启用账号</span>
              <Toggle
                label="启用账号"
                checked={!edit.disabled}
                disabled={edit.id === currentId || busy}
                onChange={(enabled) => setEdit({ ...edit, disabled: !enabled })}
              />
            </div>
            <Message error>{error}</Message>
            <div className="modal-actions">
              <Button
                type="button"
                disabled={busy}
                onClick={() => setEdit(undefined)}
              >
                取消
              </Button>
              <Button primary disabled={busy} icon={<Save size={16} />}>
                保存
              </Button>
            </div>
          </form>
        </Modal>
      )}
      {confirm && (
        <Confirm
          {...confirm}
          busy={busy}
          error={error}
          onClose={() => {
            if (!busy) setConfirm(undefined);
          }}
          onConfirm={() =>
            void mutate(confirm.path, confirm.body, confirm.method)
          }
        />
      )}
      {detail && (
        <Modal title="日志详情" onClose={() => setDetail(undefined)}>
          <dl className="detail-grid">
            <dt>事件</dt>
            <dd>{(detail as AuditEvent).event}</dd>
            <dt>操作者</dt>
            <dd>{(detail as AuditEvent).actorEmail || "系统"}</dd>
            <dt>时间</dt>
            <dd>{date((detail as AuditEvent).createdAt)}</dd>
            <dt>目标</dt>
            <dd className="mono">{(detail as AuditEvent).targetId || "—"}</dd>
          </dl>
          <pre className="json-detail">
            {typeof (detail as AuditEvent).detail === "string"
              ? String((detail as AuditEvent).detail)
              : JSON.stringify((detail as AuditEvent).detail, null, 2)}
          </pre>
        </Modal>
      )}
    </>
  );
}
