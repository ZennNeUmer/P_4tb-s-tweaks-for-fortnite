use serde::Serialize;
use std::os::windows::process::CommandExt;
use std::process::Command;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::{RegKey, HKEY};

const HCU: HKEY = HKEY_CURRENT_USER;
const HKLM: HKEY = HKEY_LOCAL_MACHINE;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// ---------------------------------------------------------------------------
// Registry helpers
// ---------------------------------------------------------------------------

fn open(h: HKEY, path: &str, write: bool) -> Result<RegKey, String> {
    let flags = if write {
        winreg::enums::KEY_READ | winreg::enums::KEY_SET_VALUE
    } else {
        winreg::enums::KEY_READ
    };
    RegKey::predef(h)
        .open_subkey_with_flags(path, flags)
        .map_err(|e| format!("HK_\\{path}: {e}"))
}

fn create(h: HKEY, path: &str) -> Result<RegKey, String> {
    RegKey::predef(h)
        .create_subkey(path)
        .map(|(k, _)| k)
        .map_err(|e| format!("HK_\\{path}: {e}"))
}

fn set_dword(h: HKEY, path: &str, name: &str, val: u32) -> Result<(), String> {
    create(h, path)?
        .set_value(name, &val)
        .map_err(|e| format!("HK_\\{path}\\{name}: {e}"))
}

fn set_str(h: HKEY, path: &str, name: &str, val: &str) -> Result<(), String> {
    create(h, path)?
        .set_value(name, &val)
        .map_err(|e| format!("HK_\\{path}\\{name}: {e}"))
}

fn set_bin(h: HKEY, path: &str, name: &str, val: Vec<u8>) -> Result<(), String> {
    create(h, path)?
        .set_raw_value(name, &winreg::RegValue { bytes: val, vtype: winreg::enums::REG_BINARY })
        .map_err(|e| format!("HK_\\{path}\\{name}: {e}"))
}

fn del(h: HKEY, path: &str, name: &str) {
    if let Ok(k) = open(h, path, true) {
        let _ = k.delete_value(name);
    }
}

fn get_dword(h: HKEY, path: &str, name: &str) -> Option<u32> {
    open(h, path, false).ok()?.get_value(name).ok()
}

fn get_str(h: HKEY, path: &str, name: &str) -> Option<String> {
    open(h, path, false).ok()?.get_value(name).ok()
}

fn key_exists(h: HKEY, path: &str) -> bool {
    RegKey::predef(h)
        .open_subkey_with_flags(path, winreg::enums::KEY_READ)
        .is_ok()
}

fn delete_tree(h: HKEY, path: &str) {
    // Delete deepest subkeys first (best effort, ignores failures).
    if let Ok(k) = RegKey::predef(h).open_subkey_with_flags(path, winreg::enums::KEY_READ) {
        let subs: Vec<String> = k.enum_keys().filter_map(|s| s.ok()).collect();
        let _ = k;
        for s in subs {
            delete_tree(h, &format!("{path}\\{s}"));
        }
    }
    if let Ok(parent) = open(h, path, true) {
        let _ = parent;
    }
    // Remove the key itself via its parent.
    if let Some((parent_path, name)) = path.rsplit_once('\\') {
        if let Ok(p) = open(h, parent_path, true) {
            let _ = p.delete_subkey(name);
        }
    }
}

// ---------------------------------------------------------------------------
// Service helpers (via registry Start value + sc.exe to stop the process)
// ---------------------------------------------------------------------------

fn service_path(name: &str) -> String {
    format!(r"SYSTEM\CurrentControlSet\Services\{name}")
}

fn service_is_disabled(name: &str) -> bool {
    get_dword(HKLM, &service_path(name), "Start") == Some(4)
}

fn stop_service(name: &str) {
    let _ = Command::new("sc")
        .args(["stop", name])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

fn start_service(name: &str) {
    let _ = Command::new("sc")
        .args(["start", name])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

fn disable_service(name: &str, _restore_start: u32) -> Result<(), String> {
    let _ = stop_service(name);
    set_dword(HKLM, &service_path(name), "Start", 4)
}

fn enable_service(name: &str, restore_start: u32) -> Result<(), String> {
    set_dword(HKLM, &service_path(name), "Start", restore_start)?;
    start_service(name);
    Ok(())
}

// ---------------------------------------------------------------------------
// External command helper
// ---------------------------------------------------------------------------

fn run(prog: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(prog)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("{prog}: {e}"))?;
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    let err = String::from_utf8_lossy(&out.stderr);
    if !err.trim().is_empty() {
        text.push(' ');
        text.push_str(&err);
    }
    if !out.status.success() && text.trim().is_empty() {
        return Err(format!("{prog} завершился с ошибкой {:?}", out.status.code()));
    }
    Ok(text)
}

// ---------------------------------------------------------------------------
// Tweak table
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cat {
    Privacy,
    Ui,
    Perf,
}

impl Cat {
    pub fn id(self) -> &'static str {
        match self {
            Cat::Privacy => "privacy",
            Cat::Ui => "ui",
            Cat::Perf => "perf",
        }
    }
}

pub struct TweakDef {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub cat: Cat,
}

pub const TWEAKS: &[TweakDef] = &[
    // ------------------------- Приватность --------------------------------
    TweakDef { id: "telemetry", name: "Отключить телеметрию", desc: "Уровень диагностики 0, отключение служб DiagTrack и dmwappushservice (сбор и отправка данных)", cat: Cat::Privacy },
    TweakDef { id: "adv_id", name: "Отключить рекламный ID", desc: "Приложения больше не смогут показывать персонализированную рекламу", cat: Cat::Privacy },
    TweakDef { id: "activity", name: "Отключить журнал действий", desc: "Timeline / журнал активности не публикуется и не выгружается в облако", cat: Cat::Privacy },
    TweakDef { id: "tailored", name: "Отключить персонализированные советы", desc: "Windows не использует диагностические данные для подбора советов и рекламы", cat: Cat::Privacy },
    TweakDef { id: "recall_ai", name: "Отключить Recall / Copilot", desc: "Блокировка AI-анализа данных, Recall (Win11 24H2) и встроенного Copilot", cat: Cat::Privacy },
    TweakDef { id: "cortana", name: "Отключить Cortana и веб-поиск", desc: "Политики: Cortana, облачный и веб-поиск через Пуск", cat: Cat::Privacy },
    TweakDef { id: "location", name: "Отключить отслеживание местоположения", desc: "Политика DisableLocation — системная геолокация запрещена", cat: Cat::Privacy },
    TweakDef { id: "ink", name: "Отключить сбор рукописного ввода", desc: "Запрет неявного сбора чернил/текста и персонализации ввода", cat: Cat::Privacy },
    TweakDef { id: "wifi_sense", name: "Отключить Wi-Fi Sense", desc: "Запрет авто-подключения к открытым хотспотам и раздачи сетей", cat: Cat::Privacy },
    TweakDef { id: "start_ads", name: "Убрать рекламу и советы в меню Пуск", desc: "Отключение предложенного контента, советов и тихой установки приложений", cat: Cat::Privacy },
    // ------------------------- Интерфейс ----------------------------------
    TweakDef { id: "context_menu", name: "Классическое контекстное меню", desc: "Полное меню «открыть с помощью» и т.д. в Win11 (как в Win10). Нужен перезапуск explorer", cat: Cat::Ui },
    TweakDef { id: "widgets", name: "Убрать виджеты с панели задач", desc: "Кнопка «Виджеты» (погода/новости) удаляется с панели задач", cat: Cat::Ui },
    TweakDef { id: "bing_search", name: "Отключить Bing в поиске Пуска", desc: "Поиск в меню Пуск не ходит в интернет и не показывает подсказки Bing", cat: Cat::Ui },
    TweakDef { id: "animations", name: "Отключить анимации Windows", desc: "Режим «Для наилучшего производительности»: эффекты окон, задержка меню, тени", cat: Cat::Ui },
    TweakDef { id: "transparency", name: "Отключить прозрачность", desc: "Акрил/мамми-эффекты панели задач и окон отключаются — быстрее рендер", cat: Cat::Ui },
    // ------------------------- Производительность -------------------------
    TweakDef { id: "game_dvr", name: "Отключить Xbox Game DVR", desc: "Фоновая запись игры (Win+G, клипы) отключается — заметный FPS-буст", cat: Cat::Perf },
    TweakDef { id: "game_mode", name: "Включить игровой режим", desc: "Windows автоматически приоритизирует игры (Game Mode ON)", cat: Cat::Perf },
    TweakDef { id: "mm_profile", name: "Мультимедийный профиль для игр", desc: "NetworkThrottlingIndex=off, SystemResponsiveness=0, приоритет игр GPU/CPU", cat: Cat::Perf },
    TweakDef { id: "win32_priority", name: "Приоритет активных приложений", desc: "Win32PrioritySeparation=0x26 — максимальный буст активной игры/окна", cat: Cat::Perf },
    TweakDef { id: "nagle", name: "Отключить алгоритм Нэгла", desc: "TcpAckFrequency=1 и TCPNoDelay=1 для всех адаптеров — меньше пинг в играх", cat: Cat::Perf },
    TweakDef { id: "hibernation", name: "Отключить гибернацию", desc: "powercfg /h off — освобождает место, равное ~40-75% ОЗУ (hiberfil.sys)", cat: Cat::Perf },
    TweakDef { id: "power_plan", name: "План питания «Высокая производительность»", desc: "Активация схемы High Performance / Ultimate Performance", cat: Cat::Perf },
    TweakDef { id: "mouse_accel", name: "Отключить ускорение мыши", desc: "«Повышенная точность указателя» OFF — прямое движение 1:1 (для игр). Нужен перезапуск", cat: Cat::Perf },
    TweakDef { id: "sysmain", name: "Отключить SysMain (Superfetch)", desc: "Служба предзагрузки отключается — меньше дисковой активности на SSD", cat: Cat::Perf },
    TweakDef { id: "search_index", name: "Отключить индексацию поиска", desc: "Служба WSearch отключается — меньше фонового IO, поиск немного медленнее", cat: Cat::Perf },
    TweakDef { id: "fast_startup", name: "Отключить быстрый запуск", desc: "HiberbootEnabled=0 — корректные выключения/перезагрузки, стабильнее драйверы", cat: Cat::Perf },
    TweakDef { id: "edge_bg", name: "Убрать фоновые процессы Edge", desc: "Startup boost и фоновый режим Edge отключаются политиками", cat: Cat::Perf },
    TweakDef { id: "onedrive", name: "Отключить OneDrive", desc: "Политика DisableFileSyncNGSC + удаление автозапуска из реестра", cat: Cat::Perf },
    TweakDef { id: "delivery_opt", name: "Отключить Delivery Optimization", desc: "Windows Update не раздаёт обновления P2P-способом другим ПК", cat: Cat::Perf },
];

// ---------------------------------------------------------------------------
// Apply / revert
// ---------------------------------------------------------------------------

pub fn apply_tweak(id: &str, enable: bool) -> Result<(), String> {
    match id {
        "telemetry" => {
            if enable {
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "AllowTelemetry", 0)?;
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "DoNotShowFeedbackNotifications", 1)?;
                disable_service("DiagTrack", 2)?;
                disable_service("dmwappushservice", 3)?;
            } else {
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "AllowTelemetry", 1)?;
                del(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "DoNotShowFeedbackNotifications");
                enable_service("DiagTrack", 2)?;
                enable_service("dmwappushservice", 3)?;
            }
            Ok(())
        }
        "adv_id" => {
            if enable {
                set_dword(HCU, r"Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo", "Enabled", 0)
            } else {
                set_dword(HCU, r"Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo", "Enabled", 1)
            }
        }
        "activity" => {
            if enable {
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\System", "EnableActivityFeed", 0)?;
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\System", "PublishUserActivities", 0)?;
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\System", "UploadUserActivities", 0)
            } else {
                del(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\System", "EnableActivityFeed");
                del(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\System", "PublishUserActivities");
                del(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\System", "UploadUserActivities");
                Ok(())
            }
        }
        "tailored" => {
            if enable {
                set_dword(HCU, r"Software\Policies\Microsoft\Windows\CloudContent", "DisableTailoredExperiencesWithDiagnosticData", 1)?;
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\CloudContent", "DisableTailoredExperiencesWithDiagnosticData", 1)
            } else {
                del(HCU, r"Software\Policies\Microsoft\Windows\CloudContent", "DisableTailoredExperiencesWithDiagnosticData");
                del(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\CloudContent", "DisableTailoredExperiencesWithDiagnosticData");
                Ok(())
            }
        }
        "recall_ai" => {
            if enable {
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "DisableAIDataAnalysis", 1)?;
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "AllowRecallEnablement", 0)?;
                set_dword(HCU, r"Software\Policies\Microsoft\Windows\WindowsCopilot", "TurnOffWindowsCopilot", 1)
            } else {
                del(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "DisableAIDataAnalysis");
                del(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "AllowRecallEnablement");
                del(HCU, r"Software\Policies\Microsoft\Windows\WindowsCopilot", "TurnOffWindowsCopilot");
                Ok(())
            }
        }
        "cortana" => {
            if enable {
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "AllowCortana", 0)?;
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "AllowCloudSearch", 0)?;
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "ConnectedSearchUseWeb", 0)
            } else {
                del(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "AllowCortana");
                del(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "AllowCloudSearch");
                del(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "ConnectedSearchUseWeb");
                Ok(())
            }
        }
        "location" => {
            if enable {
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\LocationAndSensors", "DisableLocation", 1)
            } else {
                del(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\LocationAndSensors", "DisableLocation");
                Ok(())
            }
        }
        "ink" => {
            if enable {
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\InputPersonalization", "RestrictImplicitInkCollection", 1)?;
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\InputPersonalization", "RestrictImplicitTextCollection", 1)?;
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\InputPersonalization", "AllowInputPersonalization", 0)?;
                set_dword(HCU, r"Software\Microsoft\InputPersonalization", "RestrictImplicitInkCollection", 1)?;
                set_dword(HCU, r"Software\Microsoft\InputPersonalization", "RestrictImplicitTextCollection", 1)?;
                set_dword(HCU, r"Software\Microsoft\InputPersonalization\TrainedDataStore", "HarvestContacts", 0)
            } else {
                del(HKLM, r"SOFTWARE\Policies\Microsoft\InputPersonalization", "RestrictImplicitInkCollection");
                del(HKLM, r"SOFTWARE\Policies\Microsoft\InputPersonalization", "RestrictImplicitTextCollection");
                del(HKLM, r"SOFTWARE\Policies\Microsoft\InputPersonalization", "AllowInputPersonalization");
                del(HCU, r"Software\Microsoft\InputPersonalization", "RestrictImplicitInkCollection");
                del(HCU, r"Software\Microsoft\InputPersonalization", "RestrictImplicitTextCollection");
                del(HCU, r"Software\Microsoft\InputPersonalization\TrainedDataStore", "HarvestContacts");
                Ok(())
            }
        }
        "wifi_sense" => {
            if enable {
                set_dword(HKLM, r"SOFTWARE\Microsoft\WcmSvc\wifinetworkmanager\config", "AutoConnectAllowedOEM", 0)
            } else {
                del(HKLM, r"SOFTWARE\Microsoft\WcmSvc\wifinetworkmanager\config", "AutoConnectAllowedOEM");
                Ok(())
            }
        }
        "start_ads" => {
            let base = r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager";
            let names = [
                "SubscribedContent-338388Enabled",
                "SubscribedContent-338389Enabled",
                "SubscribedContent-338393Enabled",
                "SubscribedContent-353694Enabled",
                "SubscribedContent-353696Enabled",
                "SystemPaneSuggestionsEnabled",
                "SilentInstalledAppsEnabled",
                "PreInstalledAppsEnabled",
                "OemPreInstalledAppsEnabled",
            ];
            if enable {
                for n in names {
                    set_dword(HCU, base, n, 0)?;
                }
                Ok(())
            } else {
                for n in names {
                    del(HCU, base, n);
                }
                Ok(())
            }
        }
        "context_menu" => {
            let clsid = r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32";
            if enable {
                set_str(HCU, clsid, "", "")
            } else {
                delete_tree(HCU, r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}");
                Ok(())
            }
        }
        "widgets" => {
            if enable {
                set_dword(HCU, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "TaskbarDa", 0)
            } else {
                del(HCU, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "TaskbarDa");
                Ok(())
            }
        }
        "bing_search" => {
            if enable {
                set_dword(HCU, r"Software\Microsoft\Windows\CurrentVersion\Search", "BingSearchEnabled", 0)?;
                set_dword(HCU, r"Software\Policies\Microsoft\Windows\Explorer", "DisableSearchBoxSuggestions", 1)
            } else {
                del(HCU, r"Software\Microsoft\Windows\CurrentVersion\Search", "BingSearchEnabled");
                del(HCU, r"Software\Policies\Microsoft\Windows\Explorer", "DisableSearchBoxSuggestions");
                Ok(())
            }
        }
        "animations" => {
            if enable {
                set_dword(HCU, r"Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects", "VisualFXSetting", 2)?;
                set_str(HCU, r"Control Panel\Desktop", "MenuShowDelay", "0")?;
                set_bin(HCU, r"Control Panel\Desktop", "UserPreferencesMask", vec![0x90, 0x12, 0x03, 0x80, 0x10, 0x00, 0x00, 0x00])?;
                set_dword(HCU, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "TaskbarAnimations", 0)?;
                set_dword(HCU, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "ListviewAlphaSelect", 0)?;
                set_dword(HCU, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "ListviewShadow", 0)
            } else {
                del(HCU, r"Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects", "VisualFXSetting");
                set_str(HCU, r"Control Panel\Desktop", "MenuShowDelay", "400")?;
                del(HCU, r"Control Panel\Desktop", "UserPreferencesMask");
                del(HCU, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "TaskbarAnimations");
                del(HCU, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "ListviewAlphaSelect");
                del(HCU, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "ListviewShadow");
                Ok(())
            }
        }
        "transparency" => {
            if enable {
                set_dword(HCU, r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "EnableTransparency", 0)
            } else {
                del(HCU, r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "EnableTransparency");
                Ok(())
            }
        }
        "game_dvr" => {
            if enable {
                set_dword(HCU, r"System\GameConfigStore", "GameDVR_Enabled", 0)?;
                set_dword(HCU, r"Software\Microsoft\Windows\CurrentVersion\GameDVR", "AppCaptureEnabled", 0)?;
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\GameDVR", "AllowGameDVR", 0)
            } else {
                del(HCU, r"System\GameConfigStore", "GameDVR_Enabled");
                del(HCU, r"Software\Microsoft\Windows\CurrentVersion\GameDVR", "AppCaptureEnabled");
                del(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\GameDVR", "AllowGameDVR");
                Ok(())
            }
        }
        "game_mode" => {
            if enable {
                set_dword(HCU, r"Software\Microsoft\GameBar", "AutoGameModeEnabled", 1)
            } else {
                del(HCU, r"Software\Microsoft\GameBar", "AutoGameModeEnabled");
                Ok(())
            }
        }
        "mm_profile" => {
            let base = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile";
            if enable {
                set_dword(HKLM, base, "NetworkThrottlingIndex", 0xFFFF_FFFF)?;
                set_dword(HKLM, base, "SystemResponsiveness", 0)?;
                set_dword(HKLM, &format!("{base}\\Games"), "GPU Priority", 8)?;
                set_dword(HKLM, &format!("{base}\\Games"), "Priority", 6)?;
                set_str(HKLM, &format!("{base}\\Games"), "Scheduling Category", "High")?;
                set_str(HKLM, &format!("{base}\\Games"), "SFIO Priority", "High")
            } else {
                del(HKLM, base, "NetworkThrottlingIndex");
                set_dword(HKLM, base, "SystemResponsiveness", 20)?;
                del(HKLM, &format!("{base}\\Games"), "GPU Priority");
                del(HKLM, &format!("{base}\\Games"), "Priority");
                set_str(HKLM, &format!("{base}\\Games"), "Scheduling Category", "Medium")?;
                set_str(HKLM, &format!("{base}\\Games"), "SFIO Priority", "Normal")
            }
        }
        "win32_priority" => {
            if enable {
                set_dword(HKLM, r"SYSTEM\CurrentControlSet\Control\PriorityControl", "Win32PrioritySeparation", 38)
            } else {
                del(HKLM, r"SYSTEM\CurrentControlSet\Control\PriorityControl", "Win32PrioritySeparation");
                Ok(())
            }
        }
        "nagle" => {
            let base = r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces";
            let parent = RegKey::predef(HKLM)
                .open_subkey_with_flags(base, winreg::enums::KEY_READ)
                .map_err(|e| format!("{base}: {e}"))?;
            for name in parent.enum_keys().filter_map(|s| s.ok()) {
                let path = format!("{base}\\{name}");
                if enable {
                    set_dword(HKLM, &path, "TcpAckFrequency", 1)?;
                    set_dword(HKLM, &path, "TCPNoDelay", 1)?;
                    set_dword(HKLM, &path, "TcpDelAckTicks", 0)?;
                } else {
                    del(HKLM, &path, "TcpAckFrequency");
                    del(HKLM, &path, "TCPNoDelay");
                    del(HKLM, &path, "TcpDelAckTicks");
                }
            }
            Ok(())
        }
        "hibernation" => {
            if enable {
                run("powercfg", &["/h", "off"])?;
            } else {
                run("powercfg", &["/h", "on"])?;
            }
            Ok(())
        }
        "power_plan" => {
            if enable {
                // Try High Performance directly; if missing, duplicate Ultimate Performance.
                if run("powercfg", &["-setactive", "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c"]).is_err() {
                    let out = run("powercfg", &["-duplicatescheme", "e9a42b02-d5df-448d-aa66-1e5d6d7e5f5f"])?;
                    let guid = out
                        .split_whitespace()
                        .find(|t| t.len() == 36 && t.matches('-').count() == 4)
                        .ok_or_else(|| format!("не удалось создать схему: {out}"))?;
                    run("powercfg", &["-setactive", guid])?;
                }
            } else {
                run("powercfg", &["-setactive", "381b4222-f694-41f0-9685-ff5bb260df2e"])?; // Balanced
            }
            Ok(())
        }
        "mouse_accel" => {
            if enable {
                set_str(HCU, r"Control Panel\Mouse", "MouseSpeed", "0")?;
                set_str(HCU, r"Control Panel\Mouse", "MouseThreshold1", "0")?;
                set_str(HCU, r"Control Panel\Mouse", "MouseThreshold2", "0")
            } else {
                set_str(HCU, r"Control Panel\Mouse", "MouseSpeed", "1")?;
                set_str(HCU, r"Control Panel\Mouse", "MouseThreshold1", "6")?;
                set_str(HCU, r"Control Panel\Mouse", "MouseThreshold2", "10")
            }
        }
        "sysmain" => {
            if enable {
                disable_service("SysMain", 2)
            } else {
                enable_service("SysMain", 2)
            }
        }
        "search_index" => {
            let path = service_path("WSearch");
            if enable {
                disable_service("WSearch", 2)?;
                set_dword(HKLM, &path, "DelayedAutostart", 0)
            } else {
                enable_service("WSearch", 2)?;
                set_dword(HKLM, &path, "DelayedAutostart", 1)
            }
        }
        "fast_startup" => {
            if enable {
                set_dword(HKLM, r"SYSTEM\CurrentControlSet\Control\Session Manager\Power", "HiberbootEnabled", 0)
            } else {
                del(HKLM, r"SYSTEM\CurrentControlSet\Control\Session Manager\Power", "HiberbootEnabled");
                Ok(())
            }
        }
        "edge_bg" => {
            let base = r"SOFTWARE\Policies\Microsoft\Edge";
            if enable {
                set_dword(HKLM, base, "StartupBoostEnabled", 0)?;
                set_dword(HKLM, base, "BackgroundModeEnabled", 0)
            } else {
                del(HKLM, base, "StartupBoostEnabled");
                del(HKLM, base, "BackgroundModeEnabled");
                Ok(())
            }
        }
        "onedrive" => {
            if enable {
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\OneDrive", "DisableFileSyncNGSC", 1)?;
                del(HCU, r"Software\Microsoft\Windows\CurrentVersion\Run", "OneDrive");
                Ok(())
            } else {
                del(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\OneDrive", "DisableFileSyncNGSC");
                Ok(())
            }
        }
        "delivery_opt" => {
            if enable {
                set_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization\Config", "DODownloadMode", 0)
            } else {
                del(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization\Config", "DODownloadMode");
                Ok(())
            }
        }
        _ => Err(format!("неизвестный тик: {id}")),
    }
}

// ---------------------------------------------------------------------------
// Current state ("true" = tweak applied, "false" = default, None = unknown)
// ---------------------------------------------------------------------------

pub fn tweak_state(id: &str) -> Option<bool> {
    match id {
        "telemetry" => {
            let pol = get_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\DataCollection", "AllowTelemetry") == Some(0);
            Some(pol && service_is_disabled("DiagTrack"))
        }
        "adv_id" => get_dword(HCU, r"Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo", "Enabled").map(|v| v == 0),
        "activity" => {
            let a = get_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\System", "EnableActivityFeed") == Some(0);
            let p = get_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\System", "PublishUserActivities") == Some(0);
            Some(a && p)
        }
        "tailored" => get_dword(HCU, r"Software\Policies\Microsoft\Windows\CloudContent", "DisableTailoredExperiencesWithDiagnosticData").map(|v| v == 1),
        "recall_ai" => get_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI", "DisableAIDataAnalysis").map(|v| v == 1),
        "cortana" => get_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\Windows Search", "AllowCortana").map(|v| v == 0),
        "location" => get_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\LocationAndSensors", "DisableLocation").map(|v| v == 1),
        "ink" => get_dword(HKLM, r"SOFTWARE\Policies\Microsoft\InputPersonalization", "AllowInputPersonalization").map(|v| v == 0),
        "wifi_sense" => get_dword(HKLM, r"SOFTWARE\Microsoft\WcmSvc\wifinetworkmanager\config", "AutoConnectAllowedOEM").map(|v| v == 0),
        "start_ads" => {
            let base = r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager";
            let a = get_dword(HCU, base, "SystemPaneSuggestionsEnabled") == Some(0);
            let b = get_dword(HCU, base, "SubscribedContent-338388Enabled") == Some(0);
            Some(a && b)
        }
        "context_menu" => {
            let clsid = r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32";
            Some(key_exists(HCU, clsid) && get_str(HCU, clsid, "").is_some())
        }
        "widgets" => get_dword(HCU, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "TaskbarDa").map(|v| v == 0),
        "bing_search" => {
            let b = get_dword(HCU, r"Software\Microsoft\Windows\CurrentVersion\Search", "BingSearchEnabled") == Some(0);
            let d = get_dword(HCU, r"Software\Policies\Microsoft\Windows\Explorer", "DisableSearchBoxSuggestions") == Some(1);
            Some(b && d)
        }
        "animations" => get_dword(HCU, r"Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects", "VisualFXSetting").map(|v| v == 2),
        "transparency" => get_dword(HCU, r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "EnableTransparency").map(|v| v == 0),
        "game_dvr" => {
            let a = get_dword(HCU, r"System\GameConfigStore", "GameDVR_Enabled") == Some(0);
            let b = get_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\GameDVR", "AllowGameDVR") == Some(0);
            Some(a && b)
        }
        "game_mode" => get_dword(HCU, r"Software\Microsoft\GameBar", "AutoGameModeEnabled").map(|v| v == 1),
        "mm_profile" => get_dword(HKLM, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile", "NetworkThrottlingIndex").map(|v| v == 0xFFFF_FFFF),
        "win32_priority" => get_dword(HKLM, r"SYSTEM\CurrentControlSet\Control\PriorityControl", "Win32PrioritySeparation").map(|v| v == 38),
        "nagle" => {
            let base = r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces";
            let parent = RegKey::predef(HKLM).open_subkey_with_flags(base, winreg::enums::KEY_READ).ok()?;
            let mut found = 0usize;
            let mut on = 0usize;
            for name in parent.enum_keys().filter_map(|s| s.ok()) {
                found += 1;
                if get_dword(HKLM, &format!("{base}\\{name}"), "TcpAckFrequency") == Some(1) {
                    on += 1;
                }
            }
            if found == 0 { None } else { Some(on == found) }
        }
        "hibernation" => {
            match std::fs::metadata("C:\\hiberfil.sys") {
                Ok(_) => Some(false),        // file exists -> hibernation ON -> tweak OFF
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(true),
                Err(_) => None,              // likely no permission
            }
        }
        "power_plan" => {
            let out = run("powercfg", &["/getactivescheme"]).ok()?;
            Some(out.contains("8c5e7fda") || out.contains("e9a42b02"))
        }
        "mouse_accel" => get_str(HCU, r"Control Panel\Mouse", "MouseSpeed").map(|v| v == "0"),
        "sysmain" => Some(service_is_disabled("SysMain")),
        "search_index" => Some(service_is_disabled("WSearch")),
        "fast_startup" => get_dword(HKLM, r"SYSTEM\CurrentControlSet\Control\Session Manager\Power", "HiberbootEnabled").map(|v| v == 0),
        "edge_bg" => get_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Edge", "StartupBoostEnabled").map(|v| v == 0),
        "onedrive" => get_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\OneDrive", "DisableFileSyncNGSC").map(|v| v == 1),
        "delivery_opt" => get_dword(HKLM, r"SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization\Config", "DODownloadMode").map(|v| v == 0),
        _ => None,
    }
}

pub fn is_admin() -> bool {
    RegKey::predef(HKLM)
        .open_subkey_with_flags("SOFTWARE", winreg::enums::KEY_SET_VALUE)
        .is_ok()
}

#[derive(Serialize, Clone)]
pub struct TweakInfo {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub category: String,
    pub state: String, // "on" | "off" | "unknown"
}

pub fn list_tweaks() -> Vec<TweakInfo> {
    TWEAKS
        .iter()
        .map(|t| TweakInfo {
            id: t.id.to_string(),
            name: t.name.to_string(),
            desc: t.desc.to_string(),
            category: t.cat.id().to_string(),
            state: match tweak_state(t.id) {
                Some(true) => "on",
                Some(false) => "off",
                None => "unknown",
            }
            .to_string(),
        })
        .collect()
}

#[derive(Serialize, Clone)]
pub struct TweakResult {
    pub id: String,
    pub ok: bool,
    pub message: String,
}

pub fn apply_all(enable: bool) -> Vec<TweakResult> {
    TWEAKS
        .iter()
        .map(|t| match apply_tweak(t.id, enable) {
            Ok(()) => TweakResult { id: t.id.to_string(), ok: true, message: String::new() },
            Err(e) => TweakResult { id: t.id.to_string(), ok: false, message: e },
        })
        .collect()
}

pub fn restart_explorer() -> Result<(), String> {
    let _ = run("taskkill", &["/f", "/im", "explorer.exe"]);
    Command::new("cmd")
        .args(["/C", "start", "explorer.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("explorer: {e}"))?;
    Ok(())
}

pub fn create_restore_point() -> Result<String, String> {
    run(
        "powershell",
        &[
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "Checkpoint-Computer -Description 'RustTweaker' -RestorePointType 'MODIFY_SETTINGS'",
        ],
    )
    .map(|_| "Точка восстановления создана".to_string())
}
