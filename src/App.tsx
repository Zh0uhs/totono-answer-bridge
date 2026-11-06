import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  BadgeCheck,
  Check,
  ChevronRight,
  Clipboard,
  Copy,
  Download,
  Expand,
  FileSearch,
  FolderOpen,
  Gamepad2,
  LoaderCircle,
  Maximize2,
  Minimize2,
  Minus,
  Pin,
  PinOff,
  RefreshCw,
  ScanSearch,
  Search,
  ShieldCheck,
  Sparkles,
  Square,
  TerminalSquare,
  X,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

type PathInfo = {
  gameDir: string;
  saveDir: string;
  gameRunning: boolean;
  nssExists: boolean;
  valid: boolean;
  message: string;
};

type AnswerItem = {
  index: number;
  question: string;
  options: string[];
  selectedIndex: number;
  answer: string;
  internalValue: number;
};

type ExtractionResult = {
  answers: AnswerItem[];
  rawValues: number[];
  elapsedMs: number;
};

type StatusEvent = {
  stage: string;
  message: string;
  progress: number;
};

const initialStatus: StatusEvent = {
  stage: "idle",
  message: "等待读取",
  progress: 0,
};

function App() {
  const [paths, setPaths] = useState<PathInfo | null>(null);
  const [selectedGameDir, setSelectedGameDir] = useState("");
  const [answers, setAnswers] = useState<AnswerItem[]>([]);
  const [rawValues, setRawValues] = useState<number[]>([]);
  const [status, setStatus] = useState<StatusEvent>(initialStatus);
  const [isReading, setIsReading] = useState(false);
  const [isPinned, setIsPinned] = useState(true);
  const [isCompact, setIsCompact] = useState(false);
  const [query, setQuery] = useState("");
  const [error, setError] = useState("");
  const [justCopied, setJustCopied] = useState(false);
  const resultTop = useRef<HTMLDivElement>(null);
  const autoReadStarted = useRef(false);

  useEffect(() => {
    const unlistenPromise = listen<StatusEvent>("extraction-status", (event) => {
      setStatus(event.payload);
    });
    invoke<PathInfo>("detect_paths", { gameDir: null })
      .then((info) => {
        setPaths(info);
        if (info.valid) setSelectedGameDir(info.gameDir);
      })
      .catch((reason) => setError(String(reason)));
    invoke("set_always_on_top", { value: true }).catch(() => undefined);
    return () => {
      unlistenPromise.then((unlisten) => unlisten()).catch(() => undefined);
    };
  }, []);

  const filteredAnswers = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return answers;
    return answers.filter(
      (item) =>
        String(item.index).includes(needle) ||
        item.question.toLowerCase().includes(needle) ||
        item.answer.toLowerCase().includes(needle),
    );
  }, [answers, query]);

  const chooseGame = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "选择《君与彼女与彼女之恋》游戏根目录",
    });
    if (typeof selected !== "string") return;
    setSelectedGameDir(selected);
    setError("");
    const info = await invoke<PathInfo>("detect_paths", { gameDir: selected });
    setPaths(info);
  };

  const refreshPaths = async () => {
    try {
      setError("");
      const info = await invoke<PathInfo>("detect_paths", {
        gameDir: selectedGameDir || null,
      });
      setPaths(info);
      if (info.valid) setSelectedGameDir(info.gameDir);
    } catch (reason) {
      setError(String(reason));
    }
  };

  const startExtraction = async () => {
    const gameDir = selectedGameDir || paths?.gameDir || "";
    if (!gameDir) {
      setError("请先选择游戏根目录。");
      return;
    }

    setIsReading(true);
    setError("");
    setAnswers([]);
    setRawValues([]);
    setQuery("");
    setStatus({ stage: "starting", message: "正在连接运行中的游戏", progress: 5 });

    try {
      const result = await invoke<ExtractionResult>("extract_answers", { gameDir });
      setAnswers(result.answers);
      setRawValues(result.rawValues);
      setStatus({
        stage: "complete",
        message: `读取完成，用时 ${(result.elapsedMs / 1000).toFixed(1)} 秒`,
        progress: 100,
      });
      await invoke<PathInfo>("detect_paths", { gameDir }).then(setPaths);
      requestAnimationFrame(() =>
        resultTop.current?.scrollIntoView({ behavior: "smooth", block: "start" }),
      );
    } catch (reason) {
      setError(String(reason));
      setStatus({
        stage: "error",
        message: "读取失败，游戏与存档未被修改",
        progress: 100,
      });
    } finally {
      setIsReading(false);
    }
  };

  const togglePin = async () => {
    const next = !isPinned;
    await invoke("set_always_on_top", { value: next });
    setIsPinned(next);
  };

  const toggleCompact = async () => {
    const next = !isCompact;
    await invoke("set_compact_mode", { compact: next });
    setIsCompact(next);
  };

  const copyAnswers = async () => {
    const text = answers
      .map(
        (item) =>
          `问题${String(item.index).padStart(2, "0")}：${item.question}\n内部值：${item.internalValue}\n答案：${item.answer}`,
      )
      .join("\n\n");
    await navigator.clipboard.writeText(text);
    setJustCopied(true);
    window.setTimeout(() => setJustCopied(false), 1500);
  };

  const exportAnswers = async () => {
    const target = await save({
      title: "保存答案",
      defaultPath: "Totono答案.txt",
      filters: [{ name: "文本文件", extensions: ["txt"] }],
    });
    if (!target) return;
    const text = answers
      .map(
        (item) =>
          `问题${String(item.index).padStart(2, "0")}：${item.question}\n内部值：${item.internalValue}\n答案：${item.answer}`,
      )
      .join("\n\n");
    await invoke("save_text_file", { path: target, content: text });
  };

  useEffect(() => {
    if (!autoReadStarted.current && paths?.valid && paths.gameRunning) {
      autoReadStarted.current = true;
      void startExtraction();
    }
  }, [paths?.gameRunning, paths?.valid]);

  const titlebarAction = (action: "minimize" | "maximize" | "close") => {
    invoke("window_action", { action }).catch(() => undefined);
  };

  const validGame = paths?.valid ?? false;
  const hasAnswers = answers.length === 30;

  return (
    <div className="fine-grid flex h-screen select-none flex-col overflow-hidden rounded-xl border border-white/[0.08] bg-[#080b0b]/95 text-zinc-100 shadow-2xl">
      <header
        data-tauri-drag-region
        className="drag-region flex h-11 shrink-0 items-center border-b border-white/[0.055] bg-black/25 px-3 backdrop-blur-xl"
      >
        <div className="flex min-w-0 items-center gap-2.5">
          <div className="grid size-7 place-items-center rounded-lg border border-primary/20 bg-primary/10 text-primary shadow-[0_0_22px_rgba(94,234,212,0.08)]">
            <Sparkles className="size-3.5" />
          </div>
          <div className="truncate text-[13px] font-semibold tracking-[0.12em] text-zinc-200">
            TOTONO <span className="font-normal text-zinc-500">ANSWER BRIDGE</span>
          </div>
          <Badge variant="secondary" className="hidden sm:inline-flex">
            v1.1.1
          </Badge>
        </div>
        <div className="ml-auto flex items-center gap-1">
          <Button
            variant="ghost"
            size="icon"
            className="no-drag size-8"
            title={isPinned ? "取消置顶" : "窗口置顶"}
            onClick={togglePin}
          >
            {isPinned ? <Pin className="size-4" /> : <PinOff className="size-4" />}
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="no-drag size-8"
            title={isCompact ? "标准窗口" : "紧凑悬浮模式"}
            onClick={toggleCompact}
          >
            {isCompact ? <Maximize2 className="size-4" /> : <Minimize2 className="size-4" />}
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="no-drag size-8"
            onClick={() => titlebarAction("minimize")}
          >
            <Minus className="size-4" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="no-drag size-8"
            onClick={() => titlebarAction("maximize")}
          >
            <Square className="size-3" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="no-drag size-8 hover:bg-red-500/15 hover:text-red-300"
            onClick={() => titlebarAction("close")}
          >
            <X className="size-4" />
          </Button>
        </div>
      </header>

      <main className={cn("answer-scroll min-h-0 flex-1 overflow-y-auto", isCompact ? "p-3" : "p-5")}>
        {!isCompact && (
          <section className="mb-5 flex items-start justify-between gap-5">
            <div>
              <div className="mb-2 flex items-center gap-2">
                <span className="rounded-full border border-primary/20 bg-primary/10 px-2.5 py-1 text-[10px] font-semibold tracking-[0.16em] text-primary">
                  LIVE MEMORY READER
                </span>
                <span className="text-xs text-zinc-600">进程只读 · 不改档</span>
              </div>
              <h1 className="text-[26px] font-semibold tracking-[-0.035em] text-zinc-50">
                在有答案的一侧，
                <span className="text-primary">继续游戏。</span>
              </h1>
              <p className="mt-2 max-w-2xl text-sm leading-6 text-zinc-500">
                游戏停在不可存档的问答阶段也能直接读取当前进程变量。无需按键、无需退出、无需重开，读取过程不修改游戏与存档。
              </p>
            </div>
            <div className="hidden shrink-0 items-center gap-2 rounded-2xl border border-white/[0.06] bg-white/[0.025] px-4 py-3 lg:flex">
              <ShieldCheck className="size-5 text-primary" />
              <div>
                <div className="text-xs font-medium text-zinc-300">自动恢复保护</div>
                <div className="mt-0.5 text-[10px] text-zinc-600">游戏与存档保持原样</div>
              </div>
            </div>
          </section>
        )}

        {!isCompact && (
          <Card className="glass mb-4 overflow-hidden">
            <CardContent className="p-0">
              <div className="grid gap-px bg-white/[0.05] lg:grid-cols-[1.35fr_1fr_auto]">
                <div className="bg-[#0b0f0f]/95 p-4">
                  <div className="mb-2 flex items-center gap-2 text-[10px] font-semibold uppercase tracking-[0.16em] text-zinc-600">
                    <Gamepad2 className="size-3.5" />
                    Game directory
                  </div>
                  <div className="truncate text-sm text-zinc-300" title={selectedGameDir}>
                    {selectedGameDir || "尚未选择游戏目录"}
                  </div>
                </div>
                <div className="bg-[#0b0f0f]/95 p-4">
                  <div className="mb-2 flex items-center gap-2 text-[10px] font-semibold uppercase tracking-[0.16em] text-zinc-600">
                    <FolderOpen className="size-3.5" />
                    Save directory
                  </div>
                  <div
                    className="truncate text-sm text-zinc-300"
                    title={paths?.saveDir || "未检测到存档目录"}
                  >
                    {paths?.saveDir || "未检测到存档目录"}
                  </div>
                </div>
                <div className="flex items-center gap-2 bg-[#0b0f0f]/95 p-4">
                  <Button variant="secondary" onClick={chooseGame}>
                    <FolderOpen className="size-4" />
                    选择
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={refreshPaths}
                    title="重新检测"
                  >
                    <RefreshCw className={cn("size-4", isReading && "animate-spin")} />
                  </Button>
                </div>
              </div>
              <div className="flex flex-wrap items-center gap-2 border-t border-white/[0.05] px-4 py-3">
                <Badge variant={validGame ? "success" : "warning"}>
                  {validGame ? <BadgeCheck className="mr-1 size-3" /> : null}
                  {validGame ? "游戏目录有效" : "需要选择游戏目录"}
                </Badge>
                <Badge variant={paths?.gameRunning ? "success" : "secondary"}>
                  {paths?.gameRunning ? "检测到当前问答" : "未检测到问答数据"}
                </Badge>
                <span className="ml-auto max-w-lg truncate text-[11px] text-zinc-600">
                  {error || paths?.message}
                </span>
              </div>
            </CardContent>
          </Card>
        )}

        <div className="mb-4 flex flex-wrap items-center gap-3 rounded-2xl border border-white/[0.06] bg-black/20 p-3">
          <div className="grid size-9 shrink-0 place-items-center rounded-xl border border-primary/15 bg-primary/[0.075]">
            {isReading ? (
              <LoaderCircle className="size-4 animate-spin text-primary" />
            ) : status.stage === "complete" ? (
              <Check className="size-4 text-primary" />
            ) : (
              <ScanSearch className="size-4 text-primary" />
            )}
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex items-center justify-between gap-3">
              <span className="truncate text-sm font-medium text-zinc-300">
                {status.message}
              </span>
              <span className="text-[10px] tabular-nums text-zinc-600">
                {Math.max(0, Math.min(100, status.progress))}%
              </span>
            </div>
            <div className="mt-2 h-1 overflow-hidden rounded-full bg-white/[0.05]">
              <div
                className="h-full rounded-full bg-gradient-to-r from-teal-400 to-cyan-300 transition-all duration-500"
                style={{ width: `${Math.max(2, status.progress)}%` }}
              />
            </div>
          </div>
          <Button
            size={isCompact ? "sm" : "lg"}
            className="min-w-32"
            disabled={!validGame || isReading}
            onClick={startExtraction}
          >
            {isReading ? (
              <LoaderCircle className="size-4 animate-spin" />
            ) : (
              <FileSearch className="size-4" />
            )}
            {isReading ? "正在读取" : hasAnswers ? "重新读取" : "读取答案"}
          </Button>
        </div>

        {error && (
          <div className="mb-4 rounded-xl border border-red-400/15 bg-red-400/[0.06] px-4 py-3 text-sm text-red-300">
            {error}
          </div>
        )}

        {!hasAnswers && !isCompact && (
          <div className="grid gap-3 lg:grid-cols-3">
            <Feature
              icon={<Gamepad2 className="size-4" />}
              title="保持游戏运行"
              text="问答画面中直接读取当前内存变量，不按键、不重启、不丢进度。"
            />
            <Feature
              icon={<ShieldCheck className="size-4" />}
              title="只读且不改档"
              text="不替换 nss-1.npa，不写入游戏目录，也不会移动或覆盖存档。"
            />
            <Feature
              icon={<Pin className="size-4" />}
              title="悬浮边玩边看"
              text="答案窗口可置顶、切换紧凑模式，不必在游戏和文档间切换。"
            />
          </div>
        )}

        {hasAnswers && (
          <section ref={resultTop} className="animate-fade-up">
            <div className="mb-3 flex flex-wrap items-center gap-2">
              <div>
                <div className="flex items-center gap-2">
                  <h2 className="text-lg font-semibold tracking-tight text-zinc-100">
                    当前运行答案
                  </h2>
                  <Badge variant="success">30 / 30</Badge>
                </div>
                <div className="mt-1 text-[11px] text-zinc-600">
                  内部值：{rawValues.join(", ")}
                </div>
              </div>
              <div className="ml-auto flex items-center gap-2">
                <div className="relative hidden sm:block">
                  <Search className="absolute left-3 top-1/2 size-3.5 -translate-y-1/2 text-zinc-600" />
                  <Input
                    value={query}
                    onChange={(event) => setQuery(event.target.value)}
                    placeholder="搜索题目或答案"
                    className="h-9 w-48 pl-9 text-xs"
                  />
                </div>
                <Button variant="secondary" size="sm" onClick={copyAnswers}>
                  {justCopied ? <Clipboard className="size-3.5" /> : <Copy className="size-3.5" />}
                  {justCopied ? "已复制" : "复制全部"}
                </Button>
                <Button variant="secondary" size="sm" onClick={exportAnswers}>
                  <Download className="size-3.5" />
                  导出 TXT
                </Button>
              </div>
            </div>

            <div className={cn("grid gap-2.5", isCompact ? "grid-cols-1" : "xl:grid-cols-2")}>
              {filteredAnswers.map((item) => (
                <Card
                  key={item.index}
                  className="group overflow-hidden border-white/[0.055] bg-[#0b1010]/80 transition-colors hover:border-primary/20 hover:bg-[#0d1313]"
                >
                  <CardContent className="p-0">
                    <div className="flex items-start gap-3 p-4">
                      <div className="grid size-9 shrink-0 place-items-center rounded-xl border border-white/[0.06] bg-white/[0.035] text-[11px] font-semibold tabular-nums text-zinc-500 transition-colors group-hover:border-primary/20 group-hover:text-primary">
                        {String(item.index).padStart(2, "0")}
                      </div>
                      <div className="min-w-0 flex-1">
                        <div className="text-[13px] leading-5 text-zinc-400">
                          {item.question}
                        </div>
                        <div className="mt-2 flex items-center gap-2">
                          <span className="font-rounded inline-flex size-6 items-center justify-center rounded-lg bg-primary text-[11px] font-bold text-primary-foreground">
                            {String.fromCharCode(65 + item.selectedIndex)}
                          </span>
                          <span className="text-sm font-medium text-zinc-100">
                            {item.answer}
                          </span>
                        </div>
                      </div>
                      <Badge variant="secondary" className="shrink-0 tabular-nums">
                        内部值 {item.internalValue}
                      </Badge>
                    </div>
                  </CardContent>
                </Card>
              ))}
            </div>

            {filteredAnswers.length === 0 && (
              <div className="rounded-2xl border border-dashed border-white/[0.08] px-6 py-12 text-center text-sm text-zinc-600">
                没有匹配的题目
              </div>
            )}
          </section>
        )}
      </main>

      <footer
        className={cn(
          "flex h-8 shrink-0 items-center gap-2 border-t border-white/[0.05] bg-black/30 px-3 text-[10px] text-zinc-600",
          isCompact && "hidden",
        )}
      >
        <TerminalSquare className="size-3" />
        <span>当前游戏进程只读提取</span>
        <ChevronRight className="size-3 text-zinc-700" />
        <span className="truncate">答案会始终保留在本地</span>
        <span className="ml-auto flex items-center gap-1.5">
          <span className={cn("size-1.5 rounded-full", validGame ? "bg-emerald-400" : "bg-amber-400")} />
          {validGame ? "READY" : "SETUP REQUIRED"}
        </span>
      </footer>
    </div>
  );
}

function Feature({
  icon,
  title,
  text,
}: {
  icon: React.ReactNode;
  title: string;
  text: string;
}) {
  return (
    <Card className="border-white/[0.055] bg-white/[0.018]">
      <CardHeader className="flex-row items-center gap-3 p-4 pb-2">
        <div className="grid size-8 place-items-center rounded-lg border border-white/[0.06] bg-white/[0.035] text-primary">
          {icon}
        </div>
        <CardTitle className="text-sm">{title}</CardTitle>
      </CardHeader>
      <CardContent className="px-4 pb-4 text-xs leading-5 text-zinc-600">{text}</CardContent>
    </Card>
  );
}

export default App;
