!macro NSIS_HOOK_POSTINSTALL
  ; Keep an existing user's login preference pointing at the renamed executable.
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "PDFConverterTool"
  ${If} $0 != ""
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "PDFConverterTool" '"$INSTDIR\AlexPDF.exe" --autostart'
  ${EndIf}
  IfFileExists "$APPDATA\com.wordtopdf.converter\default_output_dir.txt" output_done
  StrCpy $0 "$DOCUMENTS\AlexPDF"
  IfSilent save_output
  nsDialogs::SelectFolderDialog "选择默认输出目录" "$0"
  Pop $1
  StrCmp $1 "error" save_output
  StrCmp $1 "" save_output
  StrCpy $0 $1
  save_output:
  CreateDirectory "$0"
  CreateDirectory "$APPDATA\com.wordtopdf.converter"
  FileOpen $2 "$APPDATA\com.wordtopdf.converter\default_output_dir.txt" w
  FileWriteUTF16LE /BOM $2 "$0"
  FileClose $2
  output_done:
!macroend
