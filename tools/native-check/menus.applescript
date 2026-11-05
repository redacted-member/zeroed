tell application "System Events"
  tell process "carvestudio"
    set out to ""
    repeat with mbi in (menu bar items 2 thru -1 of menu bar 1)
      set out to out & "== " & name of mbi & linefeed
      repeat with mi in (menu items of menu 1 of mbi)
        set n to name of mi
        if n is missing value then
          set out to out & "  ---" & linefeed
        else
          set e to enabled of mi
          set sc to ""
          try
            set sc to value of attribute "AXMenuItemCmdChar" of mi
          end try
          set out to out & "  " & n & " [" & e & "] " & sc & linefeed
          try
            repeat with sub in (menu items of menu 1 of mi)
              set out to out & "     > " & name of sub & " [" & enabled of sub & "]" & linefeed
            end repeat
          end try
        end if
      end repeat
    end repeat
    return out
  end tell
end tell
