use serde::{Deserialize, Serialize};
use std::{
    ffi::c_void,
    fs,
    mem::size_of,
    path::{Path, PathBuf},
    process::Command,
    time::{Instant, SystemTime},
};
use tauri::{AppHandle, Emitter, LogicalSize, Manager};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
const PROCESS_VM_READ: u32 = 0x0010;
const MEM_COMMIT: u32 = 0x1000;
const PAGE_NOACCESS: u32 = 0x01;
const PAGE_GUARD: u32 = 0x100;
const Q_VARIABLE_COUNT: usize = 30;

// The quiz presents these questions in a different order from the internal
// #miyuki_chara_q array used by the game's script.
const RUNTIME_INDEX_BY_QUESTION: [usize; Q_VARIABLE_COUNT] = [
    0, 1, 5, 17, 28, 12, 3, 27, 24, 22, 29, 9, 11, 25, 21, 4, 18, 2, 6, 15,
    19, 20, 7, 8, 23, 26, 13, 10, 16, 14,
];

#[repr(C)]
struct MemoryBasicInformation {
    base_address: *mut c_void,
    allocation_base: *mut c_void,
    allocation_protect: u32,
    region_size: usize,
    state: u32,
    protect: u32,
    memory_type: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit_handle: i32, process_id: u32) -> *mut c_void;
    fn VirtualQueryEx(
        process: *mut c_void,
        address: *const c_void,
        information: *mut MemoryBasicInformation,
        length: usize,
    ) -> usize;
    fn ReadProcessMemory(
        process: *mut c_void,
        address: *const c_void,
        buffer: *mut c_void,
        size: usize,
        bytes_read: *mut usize,
    ) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PathInfo {
    game_dir: String,
    save_dir: String,
    game_running: bool,
    nss_exists: bool,
    valid: bool,
    message: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnswerItem {
    index: usize,
    question: String,
    options: Vec<String>,
    selected_index: usize,
    answer: String,
    internal_value: usize,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtractionResult {
    answers: Vec<AnswerItem>,
    raw_values: Vec<usize>,
    elapsed_ms: u128,
}

#[derive(Clone, Serialize)]
struct StatusEvent {
    stage: String,
    message: String,
    progress: u8,
}

struct LiveReadResult {
    pid: u32,
    internal_values: [usize; Q_VARIABLE_COUNT],
}

struct Question {
    prompt: &'static str,
    options: [&'static str; 3],
}

const QUESTIONS: [Question; 30] = [
    Q("我为什么扔掉了手机？", ["因为太旧了", "因为不合我口味", "因为沾着令人讨厌的回忆"]),
    Q("我们同居了多久了？", ["１周", "２周", "３周"]),
    Q("下列食物中我最讨厌的是？", ["红烧猪肉块", "盐渍鲑鱼子", "西柚"]),
    Q("叫外卖的时候下列食物中我会选择的是？", ["寿司", "匹萨", "中华料理"]),
    Q("幼稚园的时候我的梦想是？", ["成为机器人驾驶员", "成为魔法少女", "成为大总统"]),
    Q("我的决胜一球应该是？", ["曲线球", "变化球", "叉式球"]),
    Q("你如何看待我跟女性朋友之间的关系？", ["就像游戏一样", "就像永远的机关", "就像世界纪录一样"]),
    Q("下列物品我最想要的是？", ["旅行券", "水蒸气锅", "试映会的票"]),
    Q("我在猫猫杂货买了什么？", ["猫咪治愈系ＤＶＤ", "猫咪叫声CD", "猫咪语言玩具"]),
    Q("我最擅长的料理是？", ["生姜烧肉", "烧卖", "杂烩饭"]),
    Q("我喜欢的香味是？", ["草莓味", "苹果味", "巧克力味"]),
    Q("我为什么喜欢猫？", ["肉球很可爱", "温文尔雅", "叫声很棒"]),
    Q("我最近在收集的书是？", ["魔术手册", "黑魔法指南书", "世界遗产写真集"]),
    Q("我擅长的投球是？", ["masakari 投法", "龙卷风式投法", "下勾投法"]),
    Q("我拍的一张恐吓照片是？", ["心一的睡脸", "心一的cosplay", "心一尿床的照片"]),
    Q("下列食物中我最喜欢吃的是？", ["切块蛋糕", "苹果糖", "布丁"]),
    Q("我是怎么计算消费税的？", ["心算", "死记硬背", "除以10，再取其2分之一"]),
    Q("大庭广众下不想牵手的理由是？", ["觉得害羞", "觉得有点显摆", "因为觉得别人不愿意看到"]),
    Q("我写过的一个脚本的题目是？", ["修罗场！", "修罗场！！", "修罗场！！！"]),
    Q("我讨厌的体育运动是？", ["排球", "马拉松", "网球"]),
    Q("心一层为我做过的料理是？", ["大阪烧", "煎饺", "汉堡牛肉饼"]),
    Q("上学放学路上会和心一顺便去的地方是？", ["哪里也不去", "有猫的神社", "河边"]),
    Q("我最喜欢的冰激凌是？", ["赏月大福", "西瓜冰棍", "巧克力冰棍"]),
    Q("被询问道体重的时候我的反应是？", ["严守秘密", "用视线进行威胁", "用球杆进行殴打"]),
    Q("在我写的脚本里，主人公的名字是？", ["慎一", "真一", "贞一"]),
    Q("我能接受的相关物品的限度是？", ["垫子", "床单", "等身ＰＯＰ"]),
    Q("对于物理学家埃尔温·薛定谔的思想实验选择猫的理由我是怎么认为的？", ["薛定谔对猫过敏", "薛定谔想杀死猫", "猫有一种不可思议的力量"]),
    Q("我喜欢照片的理由是？", ["照片可以留住永恒", "通过照片能够确认自己的变化", "能回忆起幸福的瞬间"]),
    Q("我为什么喜欢这条街区？", ["因为有心一在", "因为多余的东西已经消失了", "因为这里是生我养我的地方"]),
    Q("我不擅长的科目是？", ["生物", "物理", "化学"]),
];

#[allow(non_snake_case)]
const fn Q(prompt: &'static str, options: [&'static str; 3]) -> Question {
    Question { prompt, options }
}

fn emit_status(app: &AppHandle, stage: &str, message: &str, progress: u8) {
    let _ = app.emit(
        "extraction-status",
        StatusEvent {
            stage: stage.to_string(),
            message: message.to_string(),
            progress,
        },
    );
}

#[tauri::command]
fn detect_paths(game_dir: Option<String>) -> Result<PathInfo, String> {
    let game = resolve_game_dir(game_dir.map(PathBuf::from))?;
    let save = find_save_dir().ok_or_else(|| {
        "未找到存档目录。默认位置应为 %APPDATA%\\Nitroplus\\你与彼女和彼女之恋。\\1.00"
            .to_string()
    })?;
    let nss = game.join("nss-1.npa").is_file();
    let live_ready = read_live_q_values().is_ok();
    Ok(PathInfo {
        game_dir: display_path(&game),
        save_dir: display_path(&save),
        game_running: live_ready,
        nss_exists: nss,
        valid: game.join("totono.exe").is_file() && save.join("common/cqst.npf").is_file(),
        message: if live_ready {
            "游戏正在运行；可直接只读提取当前问答，无需按键、退出或重开".to_string()
        } else {
            "请在游戏进入问答画面后点击读取；工具不会启动或重启游戏".to_string()
        },
    })
}

#[tauri::command]
async fn extract_answers(app: AppHandle, _game_dir: String) -> Result<ExtractionResult, String> {
    let save = find_save_dir().ok_or_else(|| "未找到存档目录".to_string())?;
    tauri::async_runtime::spawn_blocking(move || extract_answers_live(&app, &save))
    .await
    .map_err(|error| format!("读取线程异常：{error}"))?
}

#[tauri::command]
fn window_action(app: AppHandle, action: String) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())?;
    match action.as_str() {
        "minimize" => window.minimize().map_err(|e| e.to_string()),
        "maximize" => {
            let maximized = window.is_maximized().map_err(|e| e.to_string())?;
            if maximized {
                window.unmaximize().map_err(|e| e.to_string())
            } else {
                window.maximize().map_err(|e| e.to_string())
            }
        }
        "close" => {
            window.close().map_err(|e| e.to_string())?;
            Ok(())
        }
        _ => Err("未知窗口操作".to_string()),
    }
}

#[tauri::command]
fn set_always_on_top(app: AppHandle, value: bool) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())?
        .set_always_on_top(value)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_compact_mode(app: AppHandle, compact: bool) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())?;
    let size = if compact {
        LogicalSize::new(560.0, 760.0)
    } else {
        LogicalSize::new(1040.0, 760.0)
    };
    window.set_size(size).map_err(|e| e.to_string())?;
    window.set_always_on_top(true).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_text_file(path: String, content: String) -> Result<(), String> {
    fs::write(path, content).map_err(|e| format!("写入答案文件失败：{e}"))
}

fn extract_answers_live(app: &AppHandle, save: &Path) -> Result<ExtractionResult, String> {
    let start = Instant::now();

    emit_status(app, "backup", "正在备份真实 1.00 存档", 12);
    backup_save_directory(save)?;

    emit_status(
        app,
        "scanning",
        "正在只读扫描运行中的游戏变量",
        48,
    );

    let live = read_live_q_values()?;
    let values = live.internal_values.to_vec();
    let answers = build_answers(&values)?;
    let display_values = answers.iter().map(|item| item.internal_value).collect();
    let elapsed_ms = start.elapsed().as_millis();
    emit_status(
        app,
        "complete",
        &format!("已从运行中的游戏进程读取答案，PID {}", live.pid),
        100,
    );
    Ok(ExtractionResult {
        answers,
        raw_values: display_values,
        elapsed_ms,
    })
}

fn read_live_q_values() -> Result<LiveReadResult, String> {
    let process_ids = enumerate_process_ids()?;
    if process_ids.is_empty() {
        return Err(
            "没有找到运行中的 totono.exe。请确认游戏已进入问答画面；\
             如果游戏以管理员身份运行，请也以管理员身份启动本工具。"
                .to_string(),
        );
    }

    let scanned = process_ids.len();
    for process_id in process_ids {
        match read_process_q_values(process_id) {
            Ok(internal_values) => {
                return Ok(LiveReadResult {
                    pid: process_id,
                    internal_values,
                })
            }
            Err(_) => {}
        }
    }
    Err(format!(
        "没有找到当前问答变量（已扫描 {scanned} 个进程）。请确认游戏已进入问答画面；\
         如果游戏以管理员身份运行，请也以管理员身份启动本工具。"
    ))
}

fn enumerate_process_ids() -> Result<Vec<u32>, String> {
    let mut command = Command::new("tasklist.exe");
    command.args(["/FI", "IMAGENAME eq totono.exe", "/FO", "CSV", "/NH"]);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .output()
        .map_err(|error| format!("无法调用系统进程列表：{error}"))?;
    if !output.status.success() {
        return Err("系统进程列表读取失败".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split(',').nth(1))
        .filter_map(|field| field.trim().trim_matches('"').parse::<u32>().ok())
        .collect())
}

fn read_process_q_values(process_id: u32) -> Result<[usize; Q_VARIABLE_COUNT], String> {
    let process = unsafe {
        OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
            0,
            process_id,
        )
    };
    if process.is_null() {
        return Err("OpenProcess 被拒绝".to_string());
    }

    let result = (|| {
        let mut values = [0usize; Q_VARIABLE_COUNT];
        let mut found = [false; Q_VARIABLE_COUNT];
        let mut address = 0usize;
        while address < 0x7FFF_FFFF_FFFF {
            let mut information: MemoryBasicInformation = unsafe { std::mem::zeroed() };
            let queried = unsafe {
                VirtualQueryEx(
                    process,
                    address as *const c_void,
                    &mut information,
                    size_of::<MemoryBasicInformation>(),
                )
            };
            if queried == 0 {
                break;
            }

            let base = information.base_address as usize;
            let region_size = information.region_size;
            let readable = information.state == MEM_COMMIT
                && information.protect & PAGE_NOACCESS == 0
                && information.protect & PAGE_GUARD == 0
                && region_size > 0
                && region_size <= 64 * 1024 * 1024;

            if readable {
                let mut data = vec![0u8; region_size];
                let mut bytes_read = 0usize;
                let read = unsafe {
                    ReadProcessMemory(
                        process,
                        base as *const c_void,
                        data.as_mut_ptr() as *mut c_void,
                        region_size,
                        &mut bytes_read,
                    )
                };
                if read != 0 {
                    data.truncate(bytes_read);
                    if contains_bytes(&data, b"#miyuki_chara_q/") {
                        collect_q_values(&data, &mut values, &mut found);
                    }
                }
            }

            let Some(next) = base.checked_add(region_size) else {
                break;
            };
            if next <= address {
                break;
            }
            address = next;
        }
        if found.iter().all(|item| *item) {
            Ok(values)
        } else {
            Err(format!(
                "只找到 {}/{} 个 #miyuki_chara_q 变量",
                found.iter().filter(|item| **item).count(),
                Q_VARIABLE_COUNT
            ))
        }
    })();

    unsafe {
        CloseHandle(process);
    }
    result
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(test)]
fn discover_q_values(data: &[u8]) -> Option<[usize; Q_VARIABLE_COUNT]> {
    let mut values = [0usize; Q_VARIABLE_COUNT];
    let mut found = [false; Q_VARIABLE_COUNT];
    collect_q_values(data, &mut values, &mut found);
    found.iter().all(|item| *item).then_some(values)
}

fn collect_q_values(
    data: &[u8],
    values: &mut [usize; Q_VARIABLE_COUNT],
    found: &mut [bool; Q_VARIABLE_COUNT],
) {
    for question_index in 0..Q_VARIABLE_COUNT {
        if found[question_index] {
            continue;
        }
        let needle = format!("#miyuki_chara_q/{question_index}\0").into_bytes();
        for name_offset in 64..data.len().saturating_sub(needle.len()) + 1 {
            if &data[name_offset..name_offset + needle.len()] != needle.as_slice() {
                continue;
            }

            let record_offset = name_offset - 64;
            let Some(marker_bytes) = data.get(record_offset + 24..record_offset + 28) else {
                continue;
            };
            let marker = u32::from_le_bytes(marker_bytes.try_into().unwrap());
            if marker != 0x11 && marker != 0x12 {
                continue;
            }

            let Some(numeric_bytes) = data.get(record_offset + 56..record_offset + 64) else {
                continue;
            };
            let numeric = f64::from_le_bytes(numeric_bytes.try_into().unwrap());
            let Some(value) = [0.0, 1.0, 2.0]
                .iter()
                .position(|candidate| numeric == *candidate)
            else {
                continue;
            };

            values[question_index] = value;
            found[question_index] = true;
            break;
        }
    }
}

fn backup_save_directory(save: &Path) -> Result<(), String> {
    let parent = save.parent().ok_or_else(|| "存档目录没有父目录".to_string())?;
    let name = save.file_name().ok_or_else(|| "存档目录名称无效".to_string())?;
    let backup = parent.join(format!("{}.backup", name.to_string_lossy()));
    if backup.exists() {
        return Ok(());
    }
    copy_tree(save, &backup)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|e| format!("创建备份目录失败：{e}"))?;
    for entry in fs::read_dir(source).map_err(|e| format!("读取存档目录失败：{e}"))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else {
            fs::copy(&source_path, &destination_path).map_err(|e| {
                format!("备份存档文件失败 {}：{e}", source_path.display())
            })?;
        }
    }
    Ok(())
}

fn resolve_game_dir(input: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(path) = input {
        let mut current = if path.is_file() {
            path.parent().map(Path::to_path_buf).unwrap_or(path)
        } else {
            path
        };
        for _ in 0..4 {
            if current.join("totono.exe").is_file() {
                return Ok(current);
            }
            current = current
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| "没有找到 totono.exe".to_string())?;
        }
        return Err("所选目录中没有 totono.exe".to_string());
    }

    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.to_path_buf());
        }
    }
    for base in [PathBuf::from(r"D:\game"), PathBuf::from(r"C:\game")] {
        if let Ok(entries) = fs::read_dir(base) {
            candidates.extend(
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.join("totono.exe").is_file()),
            );
        }
    }
    candidates
        .into_iter()
        .find(|path| path.join("totono.exe").is_file())
        .ok_or_else(|| "未自动找到游戏目录，请手动选择 totono.exe 所在文件夹".to_string())
}

fn find_save_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    let mut nitro_roots = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        nitro_roots.push(PathBuf::from(appdata).join("Nitroplus"));
    }
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        nitro_roots.push(
            PathBuf::from(profile)
                .join("AppData")
                .join("Roaming")
                .join("Nitroplus"),
        );
    }
    nitro_roots.sort();
    nitro_roots.dedup();

    for nitro in nitro_roots {
        if let Ok(entries) = fs::read_dir(nitro) {
            for entry in entries.flatten() {
                let base = entry.path();
                for version in ["1.00", "1.01"] {
                    let candidate = base.join(version);
                    if candidate.join("common/cqst.npf").is_file() {
                        candidates.push(candidate);
                    }
                }
            }
        }
    }
    candidates.sort_by_key(|path| {
        fs::metadata(path.join("common/cqst.npf"))
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    });
    candidates.pop()
}

fn build_answers(values: &[usize]) -> Result<Vec<AnswerItem>, String> {
    QUESTIONS
        .iter()
        .enumerate()
        .map(|(index, question)| {
            let runtime_index = RUNTIME_INDEX_BY_QUESTION[index];
            let selected = *values
                .get(runtime_index)
                .ok_or_else(|| format!("缺少第 {} 题答案", index + 1))?;
            let answer = question
                .options
                .get(selected)
                .ok_or_else(|| format!("第 {} 题内部值无效", index + 1))?;
            Ok(AnswerItem {
                index: index + 1,
                question: question.prompt.to_string(),
                options: question.options.iter().map(|item| item.to_string()).collect(),
                selected_index: selected,
                answer: (*answer).to_string(),
                internal_value: selected,
            })
        })
        .collect()
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\")
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            detect_paths,
            extract_answers,
            window_action,
            set_always_on_top,
            set_compact_mode,
            save_text_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running TOTONO Answer Bridge");
}

pub fn run_cli_if_requested() -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some("--extract-cli") {
        return false;
    }
    let start = Instant::now();
    match read_live_q_values().and_then(|live| {
        let values = live.internal_values.to_vec();
        let answers = build_answers(&values)?;
        let display_values = answers.iter().map(|item| item.internal_value).collect();
        eprintln!("[100%] 已从问答变量所在的游戏进程读取答案，PID {}", live.pid);
        Ok(ExtractionResult {
            answers,
            raw_values: display_values,
            elapsed_ms: start.elapsed().as_millis(),
        })
    }) {
        Ok(result) => match serde_json::to_string_pretty(&result) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("serialize error: {error}");
                std::process::exit(3);
            }
        },
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_all_runtime_records() {
        let mut memory = Vec::new();
        let expected = [
            2, 0, 0, 0, 0, 0, 2, 2, 1, 1, 1, 2, 2, 1, 0, 0, 0, 0, 2, 0, 2, 1, 1, 0, 0, 0,
            0, 0, 0, 2,
        ];
        for (index, value) in expected.iter().enumerate() {
            let start = memory.len();
            memory.resize(start + 96, 0);
            memory[start + 24..start + 28].copy_from_slice(&0x12u32.to_le_bytes());
            memory[start + 56..start + 64].copy_from_slice(&(*value as f64).to_le_bytes());
            let name = format!("#miyuki_chara_q/{index}\0");
            memory[start + 64..start + 64 + name.len()].copy_from_slice(name.as_bytes());
        }
        assert_eq!(discover_q_values(&memory), Some(expected));
    }

    #[test]
    fn quiz_mapping_uses_every_runtime_slot_once() {
        let mut seen = [false; Q_VARIABLE_COUNT];
        for index in RUNTIME_INDEX_BY_QUESTION {
            assert!(index < Q_VARIABLE_COUNT);
            assert!(!seen[index]);
            seen[index] = true;
        }
        assert!(seen.iter().all(|value| *value));
    }
}
