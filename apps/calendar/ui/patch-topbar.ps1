$p = 'apps\calendar\ui\calendar.slint'
$lines = Get-Content $p
$start = 163 - 1  # 0-based
$end = 286 - 1
$newBlock = @'
                // left cluster (flexible — always fits)
                HorizontalLayout {
                    x: 0px;
                    y: 0px;
                    width: parent.width;
                    height: parent.height;
                    padding-left: 10px;
                    padding-right: 10px;
                    padding-top: 9px;
                    padding-bottom: 9px;
                    spacing: 6px;

                    IconButton {
                        size: 42px;
                        clicked => { root.toggle-sidebar(); }
                        IconMenu { stroke-color: Theme.text-dim; preferred-width: 20px; preferred-height: 20px; }
                    }
                    Rectangle {
                        width: 30px;
                        VerticalLayout { alignment: center; HorizontalLayout { alignment: center; IconCalendarDays { stroke-color: Theme.accent; preferred-width: 26px; preferred-height: 26px; } } }
                    }
                    Text {
                        visible: root.width >= 620px;
                        text: "Calendar";
                        font-size: 20px;
                        font-weight: 600;
                        color: Theme.text;
                        vertical-alignment: center;
                    }
                    Rectangle {} // left stretch
                    todayBtn := Rectangle {
                        width: 64px;
                        height: 30px;
                        border-radius: 15px;
                        background: Theme.surface-2;
                        todayTouch := TouchArea { mouse-cursor: pointer; clicked => { root.today(); } }
                        VerticalLayout { alignment: center; Text { text: "Today"; font-size: 12.5px; font-weight: 600; color: Theme.text; horizontal-alignment: center; } }
                    }
                    IconButton {
                        size: 38px;
                        clicked => { root.nav-previous(); }
                        IconChevronLeft { stroke-color: Theme.text-dim; preferred-width: 18px; preferred-height: 18px; }
                    }
                    IconButton {
                        size: 38px;
                        clicked => { root.nav-next(); }
                        IconChevronRight { stroke-color: Theme.text-dim; preferred-width: 18px; preferred-height: 18px; }
                    }
                    Text {
                        visible: root.width >= 520px;
                        text: root.date-title;
                        font-size: 15px;
                        font-weight: 600;
                        color: Theme.text;
                        vertical-alignment: center;
                        horizontal-alignment: center;
                        max-width: 240px;
                        overflow: elide;
                        single-line: true;
                    }
                    Rectangle {} // right stretch
                    switcher := Rectangle {
                        width: 180px;
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
                        width: 118px;
                        height: 36px;
                        border-radius: 18px;
                        background: Theme.accent;
                        newTouch := TouchArea { mouse-cursor: pointer; clicked => { root.new-event(); } }
                        HorizontalLayout {
                            alignment: center;
                            spacing: 5px;
                            IconPlus { stroke-color: Theme.on-accent; preferred-width: 16px; preferred-height: 16px; }
                            Text { text: "Create"; font-size: 12px; font-weight: 700; color: Theme.on-accent; vertical-alignment: center; }
                        }
                    }
                    themeBtn := IconButton {
                        size: 38px;
                        clicked => { root.toggle-theme(); }
                        if Theme.dark:
                            IconSun { stroke-color: Theme.text-dim; preferred-width: 18px; preferred-height: 18px; }
                        if !Theme.dark:
                            IconMoon { stroke-color: Theme.text-dim; preferred-width: 18px; preferred-height: 18px; }
                    }
                }
            }
'@
$out = $lines[0..($start-1)] + $newBlock.Split("`n") + $lines[$end..($lines.Length-1)]
Set-Content $p ($out -join "`n") -NoNewline
Write-Output 'topbar replaced'
