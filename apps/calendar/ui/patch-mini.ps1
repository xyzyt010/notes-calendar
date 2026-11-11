$p = 'apps\calendar\ui\calendar.slint'
$lines = Get-Content $p
$start = 308   # line 309 is 1-based
$end = 341     # line 342 is 1-based (the closing '}' of the for block)
# verify anchors
if (($lines[$start] -match 'for cell\[i\] in root\.mini-days')) {
  $newBlock = @(
    '                                        for cell[i] in root.mini-days:',
    '                                            if cell.day > 0:',
    '                                                miniDay := Rectangle {',
    '                                                    col: Math.mod(i + root.mini-lead, 7);',
    '                                                    row: Math.floor((i + root.mini-lead) / 7);',
    '                                                    width: 34px;',
    '                                                    height: 26px;',
    '                                                    border-radius: 13px;',
    '                                                    background: cell.today ? Theme.accent : (cell.selected ? Theme.accent-soft : (miniTouch.has-hover ? Theme.surface-2 : transparent));',
    '',
    '                                                    miniTouch := TouchArea {',
    '                                                        width: 100%; height: 100%;',
    '                                                        mouse-cursor: pointer;',
    '                                                        clicked => { root.day-selected(cell.day); }',
    '                                                    }',
    '                                                    Text {',
    '                                                        text: cell.day;',
    '                                                        font-size: 11px;',
    '                                                        font-weight: cell.today ? 700 : 400;',
    '                                                        color: cell.today ? Theme.on-accent : (cell.selected ? Theme.accent : Theme.text);',
    '                                                        horizontal-alignment: center;',
    '                                                        vertical-alignment: center;',
    '                                                    }',
    '                                                    if cell.busy && !cell.today:',
    '                                                        Rectangle {',
    '                                                            y: parent.height - 3.5px;',
    '                                                            x: (parent.width - 3px) / 2;',
    '                                                            width: 3px;',
    '                                                            height: 3px;',
    '                                                            border-radius: 1.5px;',
    '                                                            background: Theme.text-faint;',
    '                                                        }',
    '                                                }'
  )
  $before = $lines[0..($start-1)]
  $after = $lines[$end..($lines.Length-1)]
  $out = $before + $newBlock + $after
  Set-Content $p ($out -join "`n") -NoNewline
  Write-Output 'REPLACED'
} else {
  Write-Output 'ANCHOR NOT FOUND'
  exit 1
}
