" ttykasten.vim: notes in plain Vim (8.2 or newer) or Neovim, through
" ttykasten, with no plugin manager and no fzfkasten.nvim.
"
" Put this file in ~/.vim/plugin/ or ~/.config/nvim/plugin/ (or `source` it
" from your vimrc). ttykasten draws its lists with fzf, in the terminal Vim
" runs in (Neovim: in a terminal tab), and tells Vim what to open; writes
" to notes are ttykasten's, and the buffers are reloaded after them.
"
" Commands:
"   :Ttykasten [ACTION ARGS]  an action, as on the command line; no action
"                             opens the menu. What it picks is opened here.
"   :TtykastenInsertLink      pick a note and insert [[note]] at the cursor
"
" Keys (unless g:ttykasten_no_maps is set):
"   <leader>KEY   every menu key in ttykasten's config (`ttykasten keys`),
"                 e.g. <leader>kd for a menu item with key = "kd"
"   <leader>k     alone (pause after it): the menu, with every key and what
"                 it does (g:ttykasten_menu_map; "" leaves it unmapped)
"   In markdown buffers, about the cursor's line (g:ttykasten_line_maps
"   changes these; "" leaves one unmapped):
"   gf            follow the [[link]] under the cursor (Vim's gf elsewhere)
"   <leader>kx    task done / open again
"   <leader>kX    task cancelled / open again
"   <leader>kl    task due date (tomorrow, +3d, fri, 2026-10-02; blank clears)
"   <leader>kp    turn the line into a task (adds tasks.require_tag)
"   <leader>ki    insert a link to a note
"
" #tags in markdown buffers are coloured (group ttykastenTag, linked to
" Identifier; g:ttykasten_highlight_tags = 0 turns it off).
"
" Settings: g:ttykasten_command (default "ttykasten"), g:ttykasten_no_maps,
" g:ttykasten_line_maps, g:ttykasten_menu_map, g:ttykasten_highlight_tags.
" `set autoread` saves a prompt when a list wrote to a note that is open but
" unchanged here.

if exists('g:loaded_ttykasten')
  finish
endif
let g:loaded_ttykasten = 1

" Line continuations need Vim's 'cpoptions', which `vim -u NONE` (Vi
" compatible) does not have; put the user's back at the end.
let s:cpo = &cpo
set cpo&vim

let s:cmd = get(g:, 'ttykasten_command', 'ttykasten')

" Runs ttykasten with --print where it can draw fzf, then calls Done with
" what it printed, as a list of lines. fzf draws on the terminal itself, so
" stderr can be kept for the messages, which the terminal would lose.
function! s:run(args, Done) abort
  " ttykasten may write to any note; saving this one keeps both sides' edits.
  silent! update
  let out = tempname()
  let err = tempname()
  let cmd = s:cmd . ' --print ' . a:args . ' >' . shellescape(out) . ' 2>' . shellescape(err)
  if has('nvim')
    " Neovim's :! has no terminal to draw on, so fzf gets a terminal tab.
    let tab = tabpagenr()
    tabnew
    call termopen(cmd, {'on_exit': function('s:nvim_exit', [bufnr('%'), tab, out, err, a:Done])})
    startinsert
  else
    " :! reads % # and ! as file names and the last command.
    execute 'silent !' . escape(cmd, '%#!')
    redraw!
    call s:finish(out, err, a:Done)
  endif
endfunction

function! s:nvim_exit(buf, tab, out, err, Done, job, code, event) abort
  if bufexists(a:buf)
    execute 'bwipeout!' a:buf
  endif
  if a:tab <= tabpagenr('$')
    execute 'tabnext' a:tab
  endif
  call s:finish(a:out, a:err, a:Done)
endfunction

function! s:finish(out, err, Done) abort
  let lines = filereadable(a:out) ? readfile(a:out) : []
  let errors = filereadable(a:err) ? readfile(a:err) : []
  call delete(a:out)
  call delete(a:err)
  silent! checktime
  call a:Done(lines)
  if !empty(errors)
    " Neovim redraws after the terminal tab closes, which would wipe them.
    if has('nvim')
      call timer_start(50, {-> s:warn(errors)})
    else
      call s:warn(errors)
    endif
  endif
endfunction

function! s:warn(lines) abort
  redraw
  echohl WarningMsg
  for l in a:lines
    echomsg l
  endfor
  echohl None
endfunction

" What an action printed: `PATH<tab>LINE` opens there; a link is put in the
" unnamed register; several other lines go to a scratch buffer (the
" digest); one is a message.
function! s:open(lines) abort
  if empty(a:lines)
    return
  endif
  let m = matchlist(a:lines[0], '^\(.*\)\t\(\d\+\)$')
  if !empty(m)
    execute 'edit' fnameescape(m[1])
    execute m[2]
    normal! zz
  elseif a:lines[0] =~# '^\[\['
    call setreg('"', a:lines[0])
    echo 'yanked ' . a:lines[0]
  elseif len(a:lines) > 1
    new
    setlocal buftype=nofile bufhidden=wipe noswapfile filetype=markdown
    call setline(1, a:lines)
  else
    echo a:lines[0]
  endif
endfunction

function! s:insert(lines) abort
  if empty(a:lines) || a:lines[0] ==# ''
    return
  endif
  let line = getline('.')
  let at = col('.')
  call setline('.', strpart(line, 0, at) . a:lines[0] . strpart(line, at))
  call cursor(line('.'), at + len(a:lines[0]))
endfunction

" gf on a [[link]]: ttykasten says where it points.
function! s:follow() abort
  let out = system(s:cmd . ' link-at ' . shellescape(expand('%:p')) . ' ' . col('.'), getline('.'))
  if v:shell_error == 2
    " No link under the cursor: Vim's own gf.
    try
      normal! gf
    catch
      echohl ErrorMsg | echomsg substitute(v:exception, '^Vim(normal):', '', '') | echohl None
    endtry
    return
  elseif v:shell_error
    echohl WarningMsg | echomsg trim(out) | echohl None
    return
  endif
  let targets = split(out, "\n")
  let pick = targets[0]
  if len(targets) > 1
    let choice = inputlist(['Several notes have that name:'] + map(copy(targets), 'v:key + 1 . ". " . split(v:val, "\t")[0]'))
    if choice < 1 || choice > len(targets)
      return
    endif
    let pick = targets[choice - 1]
  endif
  call s:open([pick])
endfunction

" A task action on the cursor's line, written by ttykasten, then reloaded.
function! s:line(action, ...) abort
  silent update
  let args = [s:cmd, a:action, expand('%:p'), line('.')] + a:000
  let out = system(join(map(args, 'shellescape(v:val)')))
  if v:shell_error
    echohl WarningMsg | echomsg trim(out) | echohl None
    return
  endif
  let view = winsaveview()
  silent edit
  call winrestview(view)
  redraw
  echo trim(out)
endfunction

function! s:due() abort
  " Esc gives the cancel value, so it does not clear the date as blank does.
  let spec = inputdialog('due> ', '', "\x01")
  if spec !=# "\x01"
    call s:line('due', spec)
  endif
endfunction

command! -nargs=* Ttykasten call s:run(<q-args>, function('s:open'))
command! TtykastenInsertLink call s:run('insert-link', function('s:insert'))

" #tags, which Vim's markdown syntax and Neovim's treesitter leave plain,
" read as ttykasten reads them. A match belongs to a window, so each window
" gets one while it shows a markdown buffer.
highlight default link ttykastenTag Identifier

function! s:tag_match() abort
  let want = &filetype ==# 'markdown' && get(g:, 'ttykasten_highlight_tags', 1)
  if want && !exists('w:ttykasten_tag_match')
    let w:ttykasten_tag_match = matchadd('ttykastenTag', '#[A-Za-z0-9_-]\+')
  elseif !want && exists('w:ttykasten_tag_match')
    silent! call matchdelete(w:ttykasten_tag_match)
    unlet w:ttykasten_tag_match
  endif
endfunction

augroup ttykasten_tags
  autocmd!
  autocmd BufWinEnter,WinEnter,FileType * call s:tag_match()
augroup END

if get(g:, 'ttykasten_no_maps', 0)
  let &cpo = s:cpo
  unlet s:cpo
  finish
endif

let s:keys = systemlist(s:cmd . ' keys')
if v:shell_error
  " Not there, or older than `keys`: say so rather than map nothing quietly.
  echohl WarningMsg
  echomsg 'ttykasten.vim: `' . s:cmd . ' keys` failed, so no menu keys are mapped: ' . get(s:keys, 0, '')
  echohl None
  let s:keys = []
endif
for s:key in s:keys
  if s:key ==# ''
    continue
  endif
  execute 'nnoremap <silent> <leader>' . s:key . ' :call <SID>run(' . string(shellescape(s:key)) . ', function("<SID>open"))<CR>'
endfor
unlet! s:key s:keys

" The menu, for a pause after the keys' common start (<leader>k): there is
" no which-key here to show what follows, and the menu lists every key with
" what it does. "" leaves it unmapped.
let s:menu_map = get(g:, 'ttykasten_menu_map', '<leader>k')
if s:menu_map !=# ''
  execute 'nnoremap <silent> ' . s:menu_map . ' :call <SID>run("", function("<SID>open"))<CR>'
endif

let s:line_maps = extend({
      \ 'follow': 'gf',
      \ 'toggle': '<leader>kx',
      \ 'cancel': '<leader>kX',
      \ 'due': '<leader>kl',
      \ 'tag': '<leader>kp',
      \ 'insert_link': '<leader>ki',
      \ }, get(g:, 'ttykasten_line_maps', {}))
let s:line_calls = {
      \ 'follow': '<SID>follow()',
      \ 'toggle': '<SID>line("toggle")',
      \ 'cancel': '<SID>line("cancel")',
      \ 'due': '<SID>due()',
      \ 'tag': '<SID>line("tag-line")',
      \ 'insert_link': '<SID>run("insert-link", function("<SID>insert"))',
      \ }

function! s:map_buffer() abort
  for [name, lhs] in items(s:line_maps)
    if lhs !=# '' && has_key(s:line_calls, name)
      execute 'nnoremap <buffer> <silent> ' . lhs . ' :call ' . s:line_calls[name] . '<CR>'
    endif
  endfor
endfunction

augroup ttykasten
  autocmd!
  autocmd FileType markdown call s:map_buffer()
augroup END

let &cpo = s:cpo
unlet s:cpo
