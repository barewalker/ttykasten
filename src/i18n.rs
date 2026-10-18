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
    pub desc_link: &'static str,
    pub desc_link_daily: &'static str,
    pub desc_backlinks: &'static str,
    pub desc_tasks: &'static str,
    pub desc_inbox: &'static str,
    pub prompt_menu: &'static str,
    pub prompt_title: &'static str,
    pub prompt_task: &'static str,
    pub prompt_date: &'static str,
    pub prompt_log: &'static str,
    pub prompt_find: &'static str,
    pub prompt_line: &'static str,
    pub prompt_backlinks: &'static str,
    pub prompt_tasks: &'static str,
    pub prompt_inbox: &'static str,
    pub tasks_header: &'static str,
    pub log_header: &'static str,
    pub log_date_hint: &'static str,
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
    pub blank_line: &'static str,
    pub line_changed: &'static str,
    pub no_backlinks: &'static str,
    pub linked_into: &'static str,
    pub bad_pattern: &'static str,
    pub unknown_setting: &'static str,
    pub tasks_keys: &'static str,
    pub group_overdue: &'static str,
    pub group_today: &'static str,
    pub group_week: &'static str,
    pub group_later: &'static str,
    pub group_no_due: &'static str,
    pub group_no_tag: &'static str,
    pub all_tags: &'static str,
    pub prompt_due: &'static str,
    pub prompt_tags: &'static str,
    pub refused_cancelled: &'static str,
    pub refused_no_checkbox: &'static str,
    pub refused_done: &'static str,
    pub refused_not_open: &'static str,
    pub refused_bad_date: &'static str,
    pub refused_nothing_to_tag: &'static str,
    pub no_require_tag: &'static str,
    pub due_unchanged: &'static str,
    pub no_due_to_clear: &'static str,
    pub undone: &'static str,
    pub undo_nothing: &'static str,
    pub undo_changed: &'static str,
    pub tagged_n: &'static str,
    pub minted_n: &'static str,
    pub written: &'static str,
    pub due_unrecognised: &'static str,
    pub no_selection: &'static str,
    pub desc_capture_guided: &'static str,
    pub desc_tag: &'static str,
    pub desc_mint_ids: &'static str,
    pub desc_undo: &'static str,
    pub prompt_note: &'static str,
    pub unsaved_in_nvim: &'static str,
    pub desc_week: &'static str,
    pub desc_agenda: &'static str,
    pub desc_insert_link: &'static str,
    pub desc_note: &'static str,
    pub note_open: &'static str,
    pub note_backlinks: &'static str,
    pub note_links: &'static str,
    pub note_rename: &'static str,
    pub note_delete: &'static str,
    pub confirm_delete: &'static str,
    pub delete_cancelled: &'static str,
    pub deleted: &'static str,
    pub no_trash: &'static str,
    pub prompt_insert_link: &'static str,
    pub no_such_note: &'static str,
    pub not_in_home: &'static str,
    pub agenda_today: &'static str,
    pub agenda_header: &'static str,
    pub agenda_open_hint: &'static str,
    pub agenda_copy_hint: &'static str,
    pub agenda_refresh_hint: &'static str,
    pub agenda_empty: &'static str,
    pub copied_events: &'static str,
    pub desc_digest: &'static str,
    pub week_header: &'static str,
    pub week_digest_hint: &'static str,
    pub no_notes_in_week: &'static str,
    pub desc_grep: &'static str,
    pub desc_tags: &'static str,
    pub desc_recent: &'static str,
    pub desc_rename: &'static str,
    pub desc_links: &'static str,
    pub desc_orphans: &'static str,
    pub desc_dead_links: &'static str,
    pub desc_hubs: &'static str,
    pub romaji_hint: &'static str,
    pub bad_romaji_prefix: &'static str,
    pub prompt_grep: &'static str,
    pub prompt_tag: &'static str,
    pub prompt_recent: &'static str,
    pub prompt_rename: &'static str,
    pub prompt_template: &'static str,
    pub renamed: &'static str,
    pub rename_exists: &'static str,
    pub links_header: &'static str,
    pub links_prompt: &'static str,
    pub orphans_prompt: &'static str,
    pub dead_prompt: &'static str,
    pub dead_header: &'static str,
    pub hubs_prompt: &'static str,
    pub hubs_header: &'static str,
    pub nothing_found: &'static str,
    pub recent_today: &'static str,
}

const EN: Msgs = Msgs {
    usage: "\
Usage: ttykasten [ACTION] [ARG…]

  (none)          show the menu
  daily           open today's daily note (created from the template if missing)
  weekly          open this week's weekly note
  capture [TEXT]  append a task to the capture note
  tasks           the task list (alt-s: sort by priority, due date, added)
  inbox           checkboxes that are not tasks yet (no required tag)
                  in both lists: ctrl-x done, ctrl-d cancel, ctrl-t tag, alt-l due,
                  alt-a add, alt-u undo, alt-s sort, alt-x show closed (configurable)
  capture-guided  capture a task, then pick tags and a due date
  tag             pick lines (tab marks several) and turn them into tasks
  mint-ids        pick a note and give each tagged task in it an id
  undo            undo the last task edit (kept across runs)
  week [WEEK]     notes dated in a week (week.digest_key: its digest); WEEK is -1, 2026-W37 or a date
  agenda [WEEKS]  calendar events of a week, or of weeks like 0..1; enter opens that day's daily note
  digest [WEEK]   the week digest: notes, calendar, finished and open tasks, what's due next
  grep            search note contents (a query starting with / is romaji)
  tags            pick a tag, then a line carrying it
  recent          notes by when they were last changed
  rename          rename a note and rewrite every link to it
  note            pick a note, then open, list its backlinks or links, rename or delete it (to the trash)
  links           pick a note and see what it is linked with, a few steps deep
  orphans         notes with no link in or out
  dead-links      links that no note answers to
  hubs            notes by how many links meet there
  find            find a note and open it
  new [TITLE]     create a note from the template
  log             list daily and weekly notes (log.date_key: pick a date)
  doctor          check the setup: config, home, fzf, rg, templates, calendar, romaji
  link            pick a line of any note; copy a link to it ([[note#^id]])
  link-daily      the same, and write the link into today's daily note
  insert-link     pick a note and copy [[note]]
  backlinks       pick a line (or note) and list the lines linking to it
  config-example  print an example config file
  --version       print the version and the commit it was built from

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
    desc_link: "Link to a line (copy [[note#^id]])",
    desc_link_daily: "Link a line into today's daily note",
    desc_backlinks: "Links to a line or a note",
    desc_tasks: "Task list",
    desc_inbox: "Task inbox (untagged checkboxes)",
    prompt_menu: "ttykasten> ",
    prompt_title: "Title> ",
    prompt_task: "Task> ",
    prompt_date: "Date (YYYY-MM-DD)> ",
    prompt_log: "log> ",
    prompt_find: "find> ",
    prompt_line: "line> ",
    prompt_backlinks: "backlinks> ",
    prompt_tasks: "tasks",
    prompt_inbox: "inbox",
    tasks_header: "enter: open / alt-s: sort (priority, due, added)",
    log_header: "enter: open",
    log_date_hint: " / {}: pick a date",
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
    blank_line: "nothing on that line to link to",
    line_changed: "{} changed after it was listed; pick the line again",
    no_backlinks: "nothing links to {}",
    linked_into: "{} -> {}:{}",
    bad_pattern: "bad regex in {}: {}",
    unknown_setting: "{}: unknown setting `{}` ignored (a typo, or a config newer than this ttykasten)",
    tasks_keys: "enter open · {} done · {} cancel\n{} tag · {} due · {} add · {} undo\n{} sort · {} reverse · {} closed\n{} only a tag · {} by due · {} group",
    group_overdue: "Overdue",
    group_today: "Today",
    group_week: "Within a week",
    group_later: "Later",
    group_no_due: "No due date",
    group_no_tag: "(no tag)",
    all_tags: "(all tags)",
    prompt_due: "due (blank clears; tomorrow, +3d, fri, 2026-10-02)> ",
    prompt_tags: "tags (tab marks, enter confirms)> ",
    refused_cancelled: "that task is cancelled; reopen it first",
    refused_no_checkbox: "no checkbox on that line",
    refused_done: "that task is done; a done task is not cancelled",
    refused_not_open: "only an open task takes a due date",
    refused_bad_date: "not a due date: {}",
    refused_nothing_to_tag: "nothing to tag there",
    no_require_tag: "tasks.require_tag is not set",
    due_unchanged: "due date unchanged",
    no_due_to_clear: "no due date to clear",
    undone: "undone: {}:{}",
    undo_nothing: "nothing to undo",
    undo_changed: "{}:{} has changed since; left as it is",
    tagged_n: "tagged: {} line(s)",
    minted_n: "ids minted: {}",
    written: "{}:{}: {}",
    due_unrecognised: "didn't recognise '{}'; captured without a due date",
    no_selection: "no task selected",
    desc_capture_guided: "Capture a task with tags and a due date",
    desc_tag: "Turn lines into tasks (tab marks several)",
    desc_mint_ids: "Give every tagged task in a note an id",
    desc_undo: "Undo the last task edit",
    prompt_note: "note> ",
    unsaved_in_nvim: "{} has unsaved changes in Vim or Neovim (pid {}); save it there first",
    desc_week: "This week's notes and their digest",
    desc_agenda: "This week's calendar",
    desc_insert_link: "Link to a note (copy [[note]])",
    desc_note: "A note: open, backlinks, links, rename, delete",
    note_open: "Open",
    note_backlinks: "Lines linking to it",
    note_links: "What it is linked with",
    note_rename: "Rename (and rewrite the links to it)",
    note_delete: "Delete (to the trash)",
    confirm_delete: "delete {}? {} line(s) link to it; type yes> ",
    delete_cancelled: "not deleted",
    deleted: "{} is in the trash; {} line(s) now link to nothing",
    no_trash: "no command to send the note to the trash (gio, trash); set `trash`",
    prompt_insert_link: "link to> ",
    no_such_note: "no note named {}",
    not_in_home: "{} is not under home ({})",
    agenda_today: "today",
    agenda_header: "{} to {}{}",
    agenda_open_hint: " · enter daily note",
    agenda_copy_hint: " · {} copy",
    agenda_refresh_hint: " · {} refetch",
    agenda_empty: "nothing on the calendar {} ({} to {}){}",
    copied_events: "copied {} line(s)",
    desc_digest: "Week digest",
    week_header: "{} to {}",
    week_digest_hint: " · {} digest",
    no_notes_in_week: "no notes dated {} ({} to {})",
    desc_grep: "Search note contents (/ for romaji)",
    desc_tags: "Pick a tag, then a line carrying it",
    desc_recent: "Recently changed notes",
    desc_rename: "Rename a note and rewrite the links to it",
    desc_links: "A note's links, a few steps deep",
    desc_orphans: "Notes linked to nothing",
    desc_dead_links: "Links pointing at no note",
    desc_hubs: "Notes where links meet",
    romaji_hint: "prefix {} for romaji: {}kaigi → 会議",
    bad_romaji_prefix: "romaji.prefix must be one character other than a space and , : + ( ) ' \" \\ (got `{}`)",
    prompt_grep: "grep> ",
    prompt_tag: "tag> ",
    prompt_recent: "recent> ",
    prompt_rename: "new name> ",
    prompt_template: "template> ",
    renamed: "renamed {} to {}; links rewritten in {} note(s)",
    rename_exists: "{} already exists",
    links_header: "→ links out   ← links in   ↔ both",
    links_prompt: "links {} deep> ",
    orphans_prompt: "orphans ({})> ",
    dead_prompt: "dead links ({})> ",
    dead_header: "opens the line the link is written on",
    hubs_prompt: "hubs ({})> ",
    hubs_header: "linked to ←   links out →",
    nothing_found: "nothing found",
    recent_today: "today",
};

const JA: Msgs = Msgs {
    usage: "\
使い方: ttykasten [操作] [引数…]

  (なし)          操作の一覧を出す
  daily           今日の日誌を開く (無ければ雛形から作る)
  weekly          今週の週まとめを開く
  capture [文]    タスクを書き留める
  tasks           タスク一覧 (alt-s で優先度・期日・追加順に並べ替え)
  inbox           まだタスクになっていないチェックボックス (タグ無し)
                  一覧のキー: ctrl-x 完了, ctrl-d 取消, ctrl-t タグ, alt-l 期日,
                  alt-a 追加, alt-u 戻す, alt-s 並べ替え, alt-x 済みも表示 (設定で変更可)
  capture-guided  タスクを書き留め、タグと期日も選ぶ
  tag             行を選んで (tab で複数) タスクにする
  mint-ids        ノートを選び、タグの付いたタスクに id を付ける
  undo            直前のタスクの書き換えを戻す (起動をまたいで残る)
  week [週]       その週の日付のノート (week.digest_key で週まとめ)。週は -1, 2026-W37, 日付
  agenda [週]     その週の予定。0..1 のように複数週も可。enter でその日の日誌を開く
  digest [週]     週まとめ: ノート、予定、終えたタスクと残るタスク、翌週の期日
  grep            ノートの本文を探す (/ で始めるとローマ字)
  tags            タグを選び、そのタグの付いた行を探す
  recent          最近書き換えたノート
  rename          ノートの名前を変え、そこへのリンクをすべて直す
  note            ノートを選び、開く・被リンク・つながり・名前変更・削除 (ゴミ箱へ) から選ぶ
  links           ノートを選び、リンクでつながるノートを数段たどる
  orphans         リンクの出入りが無いノート
  dead-links      行き先のノートが無いリンク
  hubs            リンクの集まるノート
  find            ノートを探して開く
  new [題名]      雛形から新しいノートを作る
  log             日誌と週まとめの一覧 (log.date_key で日付を指定)
  doctor          設定を点検する: 設定ファイル、home、fzf、rg、テンプレート、カレンダー、ローマ字
  link            ノートの行を選び、その行へのリンク ([[note#^id]]) をコピーする
  link-daily      同じく、リンクを今日の日誌にも書く
  insert-link     ノートを選び、[[ノート]] をコピーする
  backlinks       行 (かノート) を選び、そこへリンクしている行を一覧する
  config-example  設定ファイルの見本を出す
  --version       版と、元にしたコミットを出す

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
    desc_link: "行へのリンクを取る ([[note#^id]] をコピー)",
    desc_link_daily: "行へのリンクを今日の日誌に書く",
    desc_backlinks: "行やノートへのリンクを探す",
    desc_tasks: "タスク一覧",
    desc_inbox: "受信箱 (タグの無いチェックボックス)",
    prompt_menu: "ttykasten> ",
    prompt_title: "題名> ",
    prompt_task: "タスク> ",
    prompt_date: "日付 (YYYY-MM-DD)> ",
    prompt_log: "log> ",
    prompt_find: "find> ",
    prompt_line: "line> ",
    prompt_backlinks: "backlinks> ",
    prompt_tasks: "tasks",
    prompt_inbox: "inbox",
    tasks_header: "enter: 開く / alt-s: 並べ替え (優先度・期日・追加順)",
    log_header: "enter: 開く",
    log_date_hint: " / {}: 日付を指定",
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
    blank_line: "その行にはリンクする中身が無い",
    line_changed: "{} は一覧を出した後に書き換わった。行を選び直してほしい",
    no_backlinks: "{} へのリンクは無い",
    linked_into: "{} -> {}:{}",
    bad_pattern: "{} の正規表現が不正: {}",
    unknown_setting: "{}: 知らない設定 `{}` を無視した (綴りの誤りか、設定がこの ttykasten より新しい)",
    tasks_keys: "enter 開く · {} 完了 · {} 取消\n{} タグ · {} 期日 · {} 追加 · {} 戻す\n{} 並べ替え · {} 逆順 · {} 済み\n{} タグで絞る · {} 期日で絞る · {} まとめる",
    group_overdue: "期限切れ",
    group_today: "今日",
    group_week: "1 週間以内",
    group_later: "それより先",
    group_no_due: "期日なし",
    group_no_tag: "(タグなし)",
    all_tags: "(すべてのタグ)",
    prompt_due: "期日 (空で消す。明日, +3d, 金, 2026-10-02)> ",
    prompt_tags: "タグ (tab で選ぶ、enter で決定)> ",
    refused_cancelled: "取り消したタスクは完了にできない。先に再開する",
    refused_no_checkbox: "その行にチェックボックスが無い",
    refused_done: "完了したタスクは取り消せない",
    refused_not_open: "期日を付けられるのは未完了のタスクだけ",
    refused_bad_date: "期日として読めない: {}",
    refused_nothing_to_tag: "タグを付けるものが無い",
    no_require_tag: "tasks.require_tag が設定されていない",
    due_unchanged: "期日は変わらない",
    no_due_to_clear: "消す期日が無い",
    undone: "戻した: {}:{}",
    undo_nothing: "戻すものが無い",
    undo_changed: "{}:{} はその後に書き換わったので、戻さない",
    tagged_n: "タスクにした行: {}",
    minted_n: "付けた id: {}",
    written: "{}:{}: {}",
    due_unrecognised: "'{}' を期日として読めないので、期日なしで書き留めた",
    no_selection: "タスクが選ばれていない",
    desc_capture_guided: "タグと期日も付けてタスクを書き留める",
    desc_tag: "行をタスクにする (tab で複数選べる)",
    desc_mint_ids: "ノートのタスクに id を付ける",
    desc_undo: "直前のタスクの書き換えを戻す",
    prompt_note: "note> ",
    unsaved_in_nvim: "{} は Vim か Neovim で未保存の変更がある (pid {})。先にそちらで保存してほしい",
    desc_week: "今週のノートと週まとめ",
    desc_agenda: "今週の予定",
    desc_insert_link: "ノートへのリンク ([[ノート]] をコピー)",
    desc_note: "ノートを選んで操作: 開く、被リンク、つながり、名前変更、削除",
    note_open: "開く",
    note_backlinks: "ここへリンクしている行",
    note_links: "つながっているノート",
    note_rename: "名前を変える (リンクも書き換える)",
    note_delete: "削除する (ゴミ箱へ)",
    confirm_delete: "{} を削除しますか? {} 行がここへリンクしています。yes と入力> ",
    delete_cancelled: "削除しませんでした",
    deleted: "{} をゴミ箱に送りました。{} 行のリンクが行き先を失いました",
    no_trash: "ゴミ箱に送るコマンド (gio、trash) がありません。`trash` を設定してください",
    prompt_insert_link: "link to> ",
    no_such_note: "{} という名前のノートはありません",
    not_in_home: "{} は home ({}) の中にありません",
    agenda_today: "今日",
    agenda_header: "{} 〜 {}{}",
    agenda_open_hint: " · enter その日の日誌",
    agenda_copy_hint: " · {} コピー",
    agenda_refresh_hint: " · {} 取り直す",
    agenda_empty: "{} の予定はありません ({} 〜 {}){}",
    copied_events: "{} 行コピーしました",
    desc_digest: "週まとめ",
    week_header: "{} 〜 {}",
    week_digest_hint: " · {} 週まとめ",
    no_notes_in_week: "{} ({} 〜 {}) の日付のノートは無い",
    desc_grep: "ノートの本文を探す (/ でローマ字)",
    desc_tags: "タグを選び、そのタグの付いた行を探す",
    desc_recent: "最近書き換えたノート",
    desc_rename: "ノートの名前を変え、そこへのリンクも直す",
    desc_links: "ノートのリンクのつながり",
    desc_orphans: "どこともつながっていないノート",
    desc_dead_links: "行き先のノートが無いリンク",
    desc_hubs: "リンクの集まるノート",
    romaji_hint: "{} で始めるとローマ字: {}kaigi → 会議",
    bad_romaji_prefix: "romaji.prefix は空白と , : + ( ) ' \" \\ 以外の 1 文字にしてください (今は `{}`)",
    prompt_grep: "grep> ",
    prompt_tag: "tag> ",
    prompt_recent: "recent> ",
    prompt_rename: "新しい名前> ",
    prompt_template: "雛形> ",
    renamed: "{} を {} に変えた。リンクを直したノート: {}",
    rename_exists: "{} は既にある",
    links_header: "→ リンク先   ← リンク元   ↔ 両方",
    links_prompt: "links {} deep> ",
    orphans_prompt: "orphans ({})> ",
    dead_prompt: "dead links ({})> ",
    dead_header: "リンクの書かれた行を開く",
    hubs_prompt: "hubs ({})> ",
    hubs_header: "← リンクされた数   → リンクした数",
    nothing_found: "見つからない",
    recent_today: "今日",
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
            "link" => self.desc_link,
            "link-daily" => self.desc_link_daily,
            "backlinks" => self.desc_backlinks,
            "tasks" => self.desc_tasks,
            "inbox" => self.desc_inbox,
            "capture-guided" => self.desc_capture_guided,
            "tag" => self.desc_tag,
            "mint-ids" => self.desc_mint_ids,
            "undo" => self.desc_undo,
            "week" => self.desc_week,
            "digest" => self.desc_digest,
            "agenda" => self.desc_agenda,
            "insert-link" => self.desc_insert_link,
            "note" => self.desc_note,
            "grep" => self.desc_grep,
            "tags" => self.desc_tags,
            "recent" => self.desc_recent,
            "rename" => self.desc_rename,
            "links" => self.desc_links,
            "orphans" => self.desc_orphans,
            "dead-links" => self.desc_dead_links,
            "hubs" => self.desc_hubs,
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
