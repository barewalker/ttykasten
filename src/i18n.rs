//! Messages shown on screen, in English and Japanese, chosen by `language`.
//!
//! Callers fill `{}` with [`fill`].

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    En,
    Ja,
}

pub struct Msgs {
    pub usage: &'static str,
    pub desc_daily: &'static str,
    pub desc_weekly: &'static str,
    pub desc_capture: &'static str,
    pub desc_find: &'static str,
    pub desc_new: &'static str,
    pub desc_log: &'static str,
    pub prompt_menu: &'static str,
    pub prompt_title: &'static str,
    pub prompt_task: &'static str,
    pub prompt_date: &'static str,
    pub prompt_log: &'static str,
    pub prompt_find: &'static str,
    pub log_header: &'static str,
    pub unknown_action: &'static str,
    pub menu_item_empty: &'static str,
    pub no_file_name: &'static str,
    pub opening_existing: &'static str,
    pub cannot_write: &'static str,
    pub captured: &'static str,
    pub nothing_captured: &'static str,
    pub no_capture_note: &'static str,
    pub bad_date: &'static str,
    pub future_date: &'static str,
    pub cannot_start: &'static str,
    pub failed: &'static str,
}

const EN: Msgs = Msgs {
    usage: "\
Usage: ttykasten [ACTION] [ARG…]

  (none)          show the menu
  daily           open today's daily note (created from the template if missing)
  weekly          open this week's weekly note
  capture [TEXT]  append a task to the capture note
  find            find a note and open it
  new [TITLE]     create a note from the template
  log             list daily and weekly notes (ctrl-x: pick a date)
  config-example  print an example config file

Leave TEXT or TITLE out to type it into an input line.
Keys set in the menu (e.g. key = \"kd\") also work as actions.
Config: $TTYKASTEN_CONFIG, else $XDG_CONFIG_HOME/ttykasten/config.toml.
",
    desc_daily: "Open today's daily note",
    desc_weekly: "Open this week's weekly note",
    desc_capture: "Capture a task",
    desc_find: "Find a note",
    desc_new: "New note",
    desc_log: "Daily and weekly notes",
    prompt_menu: "ttykasten> ",
    prompt_title: "Title> ",
    prompt_task: "Task> ",
    prompt_date: "Date (YYYY-MM-DD)> ",
    prompt_log: "log> ",
    prompt_find: "find> ",
    log_header: "enter: open / ctrl-x: pick a date",
    unknown_action: "unknown action: {}",
    menu_item_empty: "menu item has neither action nor command",
    no_file_name: "no usable file name in the title: {}",
    opening_existing: "{} already exists; opening it",
    cannot_write: "cannot write {}",
    captured: "{}: {}",
    nothing_captured: "empty; nothing captured",
    no_capture_note: "no capture note set: set tasks.capture_note (or the first tasks.always entry)",
    bad_date: "not a date: {}",
    future_date: "that date is in the future: {}",
    cannot_start: "cannot start {}",
    failed: "{} failed ({})",
};

const JA: Msgs = Msgs {
    usage: "\
使い方: ttykasten [操作] [引数…]

  (なし)          操作の一覧を出す
  daily           今日の日誌を開く (無ければ雛形から作る)
  weekly          今週の週まとめを開く
  capture [文]    タスクを書き留める
  find            ノートを探して開く
  new [題名]      雛形から新しいノートを作る
  log             日誌と週まとめの一覧 (ctrl-x で日付を指定)
  config-example  設定ファイルの見本を出す

文や題名を省くと、起動後の入力欄で受ける。
メニューで決めたキー (key = \"kd\" など) も操作の名前として使える。
設定: $TTYKASTEN_CONFIG、無ければ $XDG_CONFIG_HOME/ttykasten/config.toml。
",
    desc_daily: "今日の日誌を開く",
    desc_weekly: "今週の週まとめを開く",
    desc_capture: "タスクを書き留める",
    desc_find: "ノートを探して開く",
    desc_new: "新しいノートを作る",
    desc_log: "日誌と週まとめの一覧",
    prompt_menu: "ttykasten> ",
    prompt_title: "題名> ",
    prompt_task: "タスク> ",
    prompt_date: "日付 (YYYY-MM-DD)> ",
    prompt_log: "log> ",
    prompt_find: "find> ",
    log_header: "enter: 開く / ctrl-x: 日付を指定",
    unknown_action: "知らない操作: {}",
    menu_item_empty: "メニューの項目に action も command も無い",
    no_file_name: "題名からファイル名を作れない: {}",
    opening_existing: "{} は既にあるので、それを開く",
    cannot_write: "{} を書けない",
    captured: "{}: {}",
    nothing_captured: "空なので書き留めない",
    no_capture_note: "書き留める先が無い: tasks.capture_note (か tasks.always の最初) を設定する",
    bad_date: "日付として読めない: {}",
    future_date: "先の日付は開けない: {}",
    cannot_start: "{} を起動できない",
    failed: "{} が失敗した ({})",
};

impl Language {
    pub fn msgs(self) -> &'static Msgs {
        match self {
            Language::En => &EN,
            Language::Ja => &JA,
        }
    }
}

impl Msgs {
    /// The default description of an action.
    pub fn action_desc(&self, action: &str) -> Option<&'static str> {
        Some(match action {
            "daily" => self.desc_daily,
            "weekly" => self.desc_weekly,
            "capture" => self.desc_capture,
            "find" => self.desc_find,
            "new" => self.desc_new,
            "log" => self.desc_log,
            _ => return None,
        })
    }
}

/// Fills each `{}` in turn.
pub fn fill(msg: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::new();
    let mut parts = msg.split("{}");
    out.push_str(parts.next().unwrap_or(""));
    for (i, part) in parts.enumerate() {
        if let Some(a) = args.get(i) {
            out.push_str(&a.to_string());
        }
        out.push_str(part);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ACTIONS;

    #[test]
    fn filling() {
        assert_eq!(fill("{} failed ({})", &[&"fzf", &2]), "fzf failed (2)");
    }

    #[test]
    fn every_action_is_described() {
        for lang in [Language::En, Language::Ja] {
            for a in ACTIONS {
                assert!(lang.msgs().action_desc(a).is_some(), "{a}");
            }
        }
    }
}
