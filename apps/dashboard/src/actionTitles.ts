export function actionTitle(label: string, disabledReason: string) {
  return disabledReason || label;
}

export function desktopActionReason(
  desktop: boolean,
  busy: boolean,
  busyMessage = "当前操作正在执行",
) {
  if (!desktop) return "请在桌面客户端中执行此操作";
  if (busy) return busyMessage;
  return "";
}
