import { useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { AudioLines, Check, FolderOpen, Search, Trash2, X } from "lucide-react";
import { api, type ProjectListing } from "../lib/api";
import { ContinueJobButton, JobStateLabel } from "./RecentJobBadge";
import { DUB_LANGS } from "../lib/i18n";
import { facet, NO_FILTER, PROJECT_ORDERS, selectProjects, useListOrder, type ProjectFilter, type ProjectStatusFilter } from "../lib/projectList";
import SortMenu from "./SortMenu";
import { useEscapeLayer } from "../lib/escape";

// Сотни карточек с кадром-превью — сотни запросов к ffmpeg-превью; рисуем пачками.
const PAGE = 60;

export const MODE_KEYS = {
  nodub: "comp.audioNone",
  dub: "mode.dub",
  voiceover: "mode.voiceover",
  transcribe: "mode.transcribe",
  subtitles: "mode.subtitles",
  funny: "mode.funny",
} as const;

const fmtDuration = (sec: number) => {
  const s = Math.max(0, Math.round(sec));
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), r = s % 60;
  return h ? `${h}:${String(m).padStart(2, "0")}:${String(r).padStart(2, "0")}` : `${m}:${String(r).padStart(2, "0")}`;
};

const toggled = (set: ReadonlySet<string>, v: string) => {
  const next = new Set(set);
  if (next.has(v)) next.delete(v); else next.add(v);
  return next;
};

// Все проекты студии: поиск, сортировка, фильтры по режиму, языку перевода и готовности, удаление.
export default function ProjectsList({ projects, onOpen, onDelete, onClose }: {
  projects: ProjectListing[];
  onOpen: (pid: string) => void;
  onDelete: (pid: string) => Promise<void>;
  onClose: () => void;
}) {
  const { t, i18n } = useTranslation();
  const language = i18n.language || "en";
  const [filter, setFilter] = useState<ProjectFilter>(NO_FILTER);
  const [order, setOrder] = useListOrder("projects", "newest", PROJECT_ORDERS);
  const [shown, setShown] = useState(PAGE);
  const titleId = useId();

  const visible = useMemo(() => selectProjects(projects, filter, order, language), [projects, filter, order, language]);
  const modes = useMemo(() => facet(projects, "mode"), [projects]);
  const langs = useMemo(() => facet(projects, "tgt_lang"), [projects]);
  const filtered = filter.query.trim() !== "" || filter.modes.size > 0 || filter.langs.size > 0 || filter.status !== "all";

  useEscapeLayer(onClose);

  const setF = (patch: Partial<ProjectFilter>) => { setFilter((f) => ({ ...f, ...patch })); setShown(PAGE); };
  const modeLabel = (m: string) => (m in MODE_KEYS ? t(MODE_KEYS[m as keyof typeof MODE_KEYS]) : m);
  const langLabel = (code: string) => DUB_LANGS.find((l) => l.code === code)?.name ?? code.toUpperCase();
  const [openedAt] = useState(() => Date.now());
  const rtf = new Intl.RelativeTimeFormat(language, { numeric: "auto" });
  const ago = (sec: number) => {
    const min = (sec * 1000 - openedAt) / 60000;
    if (Math.abs(min) < 60) return rtf.format(Math.round(min), "minute");
    if (Math.abs(min) < 1440) return rtf.format(Math.round(min / 60), "hour");
    return rtf.format(Math.round(min / 1440), "day");
  };
  const date = (sec: number) => new Date(sec * 1000).toLocaleDateString(language, { day: "numeric", month: "short", year: "numeric" });

  const chip = (active: boolean) =>
    `inline-flex items-center gap-1 px-2 py-1 rounded-md border text-[11px] transition-colors ${active ? "border-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_12%,transparent)] text-[var(--color-text)]" : "border-[var(--color-border)] bg-[var(--color-surface-2)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`;

  return (
    <>
      <div className="fixed inset-0 z-50 grid place-items-center glass-scrim anim-fade" onClick={onClose}>
        <div role="dialog" aria-modal="true" aria-labelledby={titleId}
          className="w-[min(94vw,920px)] h-[min(88vh,820px)] flex flex-col rounded-xl glass-panel anim-pop overflow-hidden" onClick={(e) => e.stopPropagation()}>
          <div className="flex items-center gap-3 px-5 pt-4 pb-3 border-b border-[var(--color-border)]">
            <FolderOpen size={17} className="text-[var(--color-accent)] shrink-0" />
            <h2 id={titleId} className="font-semibold">{t("projects.title")}</h2>
            <span className="mono text-[11px] text-[var(--color-muted)]">{filtered ? t("projects.countFiltered", { shown: visible.length, total: projects.length }) : t("projects.count", { count: projects.length })}</span>
            <button type="button" onClick={onClose} aria-label={t("prefs.close")} title={t("prefs.close")}
              className="ml-auto p-1 rounded-md text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={16} /></button>
          </div>

          <div className="px-5 py-3 space-y-2.5 border-b border-[var(--color-border)]">
            <div className="flex items-center gap-2">
              <label className="flex-1 min-w-0 flex items-center gap-2 px-2.5 py-1.5 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-2)] focus-within:border-[var(--color-accent)]">
                <Search size={14} className="text-[var(--color-muted)] shrink-0" />
                <input autoFocus type="search" value={filter.query} onChange={(e) => setF({ query: e.target.value })}
                  placeholder={t("projects.search")} aria-label={t("projects.search")}
                  className="min-w-0 flex-1 bg-transparent text-[13px] outline-none" />
              </label>
              <SortMenu order={order} orders={PROJECT_ORDERS} onChange={setOrder} />
            </div>
            <div className="flex flex-wrap items-center gap-1.5" role="group" aria-label={t("projects.filter.status")}>
              {(["all", "done", "pending"] as const satisfies readonly ProjectStatusFilter[]).map((st) => (
                <button key={st} type="button" aria-pressed={filter.status === st} onClick={() => setF({ status: st })} className={chip(filter.status === st)}>
                  {t(`projects.filter.${st}`)}
                </button>
              ))}
              {modes.length > 1 && <span className="w-px h-4 bg-[var(--color-border)] mx-1" />}
              {modes.length > 1 && modes.map(({ value, count }) => (
                <button key={value} type="button" aria-pressed={filter.modes.has(value)} onClick={() => setF({ modes: toggled(filter.modes, value) })} className={chip(filter.modes.has(value))}>
                  {modeLabel(value)}<span className="mono text-[10px] opacity-60">{count}</span>
                </button>
              ))}
              {langs.length > 1 && <span className="w-px h-4 bg-[var(--color-border)] mx-1" />}
              {langs.length > 1 && langs.map(({ value, count }) => (
                <button key={value} type="button" aria-pressed={filter.langs.has(value)} onClick={() => setF({ langs: toggled(filter.langs, value) })} className={chip(filter.langs.has(value))}
                  title={langLabel(value)}>
                  <span className="uppercase font-semibold">{value}</span><span className="mono text-[10px] opacity-60">{count}</span>
                </button>
              ))}
              {filtered && (
                <button type="button" onClick={() => setF(NO_FILTER)} className="ml-auto inline-flex items-center gap-1 text-[11px] text-[var(--color-muted)] hover:text-[var(--color-text)]">
                  <X size={11} />{t("projects.filter.reset")}
                </button>
              )}
            </div>
          </div>

          <div className="flex-1 min-h-0 overflow-y-auto px-5 py-3">
            {visible.length === 0 ? (
              <p className="py-10 text-center text-[13px] text-[var(--color-muted)]">{projects.length === 0 ? t("projects.empty") : t("projects.noMatch")}</p>
            ) : (
              <ul className="space-y-1.5">
                {visible.slice(0, shown).map((p) => (
                  <li key={p.pid} className="group flex items-center rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] hover:bg-[var(--color-surface-2)] hover:border-[#3a414c] transition-colors">
                    <button type="button" onClick={() => onOpen(p.pid)} className="min-w-0 flex-1 flex items-center gap-3 p-1.5 text-left">
                      {p.audio_only
                        ? <div className="w-20 h-12 shrink-0 rounded-md grid place-items-center bg-[var(--color-surface-2)] text-[var(--color-accent)]"><AudioLines size={16} /></div>
                        : <img src={api.originalUrl(p.pid, Math.min(1, (p.duration || 3) / 3))} alt="" loading="lazy" className="w-20 h-12 shrink-0 rounded-md object-cover bg-black/40" />}
                      <div className="min-w-0 flex-1">
                        <div className="text-[13px] font-medium truncate">{p.video}</div>
                        <div className="mt-0.5 flex flex-wrap items-center gap-x-1.5 text-[11px] text-[var(--color-muted)]">
                          <span className="uppercase font-semibold text-[var(--color-accent-2)]" title={langLabel(p.tgt_lang)}>{p.tgt_lang}</span>
                          <span>·</span><span>{modeLabel(p.mode)}</span>
                          <span>·</span><span className="mono tabnum">{fmtDuration(p.duration)}</span>
                          {p.created != null && <><span>·</span><span>{t("projects.created", { date: date(p.created) })}</span></>}
                          <span>·</span><span>{t("projects.edited", { ago: ago(p.mtime) })}</span>
                          <JobStateLabel p={p} />
                        </div>
                      </div>
                      {p.done
                        ? <span className="shrink-0 inline-flex items-center gap-1 text-[11px] text-[var(--color-accent)]"><Check size={12} />{t("projects.filter.done")}</span>
                        : <span className="shrink-0 text-[11px] text-[var(--color-muted)]">{t("projects.filter.pending")}</span>}
                    </button>
                    <ContinueJobButton p={p} onOpen={onOpen} />
                    <button type="button" onClick={() => void onDelete(p.pid)} aria-label={t("recent.delete")} title={t("recent.delete")}
                      className="shrink-0 mx-1 p-1.5 rounded-md text-[var(--color-muted)] opacity-60 group-hover:opacity-100 focus:opacity-100 hover:text-[#ef4444] hover:bg-white/5 transition"><Trash2 size={15} /></button>
                  </li>
                ))}
              </ul>
            )}
            {visible.length > shown && (
              <button type="button" onClick={() => setShown((n) => n + PAGE)}
                className="mt-3 w-full px-3 py-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-2)] text-[12px] text-[var(--color-muted)] hover:text-[var(--color-text)] transition-colors">
                {t("projects.more", { count: Math.min(PAGE, visible.length - shown) })}
              </button>
            )}
          </div>
        </div>
      </div>
    </>
  );
}
