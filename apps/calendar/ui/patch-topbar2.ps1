$p = 'apps\calendar\ui\calendar.slint'
$lines = Get-Content $p
$start = 163   # 0-based index of the comment line
$end = 259     # 0-based index of the closing brace of the top bar Rectangle (line 260 1-based)
$newBlock = @'
                // top bar — fixed coordinates (robust across DPI/scale)
                hamburgerBtn := IconButton {
                    x: 10px;
                    y: 11px;
                    size: 40px;
                    clicked => { root.toggle-sidebar(); }
                    IconMenu { stroke-color: Theme.text-dim; preferred-width: 20px; preferred-height: 20px; }
                }
                glyphSlot := Rectangle {
                    x: 62px;
                    y: 18px;
                    width: 28px;
                    height: 28px;
                    VerticalLayout { alignment: center; HorizontalLayout { alignment: center; IconCalendarDays { stroke-color: Theme.accent; preferred-width: 22px; preferred-height: 22px; } } }
                }
                todayBtn := Rectangle {
                    x: 108px;
                    y: 17px;
                    width: 70px;
                    height: 30px;
                    border-radius: 15px;
                    background: Theme.surface-2;
                    todayTouch := TouchArea { mouse-cursor: pointer; clicked => { root.today(); } }
                    VerticalLayout { alignment: center; Text { text: "Today"; font-size: 12.5px; font-weight: 600; color: Theme.text; horizontal-alignment: center; } }
                }
                prevBtn := IconButton {
                    x: 188px;
                    y: 11px;
                    size: 40px;
                    clicked => { root.nav-previous(); }
                    IconChevronLeft { stroke-color: Theme.text-dim; preferred-width: 18px; preferred-height: 18px; }
                }
                nextBtn := IconButton {
                    x: 236px;
                    y: 11px;
                    size: 40px;
                    clicked => { root.nav-next(); }
                    IconChevronRight { stroke-color: Theme.text-dim; preferred-width: 18px; preferred-height: 18px; }
                }
                dateTitle := Text {
                    x: 290px;
                    y: 0px;
                    height: 64px;
                    vertical-alignment: center;
                    text: root.date-title;
                    font-size: 15px;
                    font-weight: 600;
                    color: Theme.text;
                    wrap: no-wrap;
                }

                switcher := Rectangle {
                    x: 690px;
                    y: 14px;
                    width: 172px;
                    height: 36px;
                    border-radius: 18px;
                    background: Theme.surface-2;
                    HorizontalLayout {
                        padding: 4px;
                        spacing: 2px;
                        SwitcherButton { label: "Day"; selected: root.view == 0; clicked => { root.view-changed(0); } }
                        SwitcherButton { label: "Week"; selected: root.view == 1; clicked => { root.view-changed(1); } }
                        SwitcherButton { label: "Month"; selected: root.view == 2; clicked => { root.view-changed(2); } }
                        SwitcherButton { label: "Year"; selected: root.view == 3; clicked => { root.view-changed(3); } }
                    }
                }
                newEventBtn := Rectangle {
                    x: 872px;
                    y: 14px;
                    width: 132px;
                    height: 36px;
                    border-radius: 18px;
                    background: Theme.accent;
                    newTouch := TouchArea { mouse-cursor: pointer; clicked => { root.new-event(); } }
                    HorizontalLayout {
                        alignment: center;
                        spacing: 6px;
                        IconPlus { stroke-color: Theme.on-accent; preferred-width: 15px; preferred-height: 15px; }
                        Text { text: "Create"; font-size: 12.5px; font-weight: 700; color: Theme.on-accent; vertical-alignment: center; }
                    }
                }
                themeBtn := IconButton {
                    x: 1016px;
                    y: 11px;
                    size: 40px;
                    clicked => { root.toggle-theme(); }
                    if Theme.dark:
                        IconSun { stroke-color: Theme.text-dim; preferred-width: 19px; preferred-height: 19px; }
                    if !Theme.dark:
                        IconMoon { stroke-color: Theme.text-dim; preferred-width: 19px; preferred-height: 19px; }
                }
            }
        }
'@
$out = $lines[0..($start-1)] + $newBlock.Split("`n") + $lines[$end..($lines.Length-1)]
Set-Content $p ($out -join "`n") -NoNewline
Write-Output 'top bar replaced with fixed coordinates'
