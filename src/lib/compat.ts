import type { Compatibility } from "./types";

// English text comes from the backend; these are the translations keyed by the stable code.
const translations: Record<string, Record<string, string>> = {
  es: {
    android_unauthorized: "Desbloquea el teléfono y toca Permitir en el aviso «¿Permitir depuración USB?». Si no aparece, desconecta el cable y vuelve a conectarlo.",
    android_offline: "El dispositivo no responde a adb. Vuelve a conectar el cable o reinicia el dispositivo.",
    android_not_booted: "El dispositivo no ha terminado de iniciarse. Inícialo con normalidad y vuelve a conectarlo.",
    android_too_old: "La duplicación de pantalla requiere Android 5.0 o posterior.",
    android_no_audio: "El reenvío de audio requiere Android 11 o posterior. El vídeo y el control funcionan con normalidad.",
    android_debugging_off: "Este teléfono está conectado, pero no responde a adb. Comprueba que la depuración USB esté activada en Opciones de desarrollador y vuelve a conectar el cable.",
    ios_not_trusted: "Desbloquea el dispositivo y toca Confiar. Si no aparece el aviso, abre Finder (macOS) o la app Dispositivos Apple (Windows) con el dispositivo conectado.",
    ios_view_only: "Duplicación solo de visualización. iOS no permite que un ordenador controle el dispositivo.",
    ios_no_driver: "Instala la app Dispositivos Apple desde Microsoft Store para identificar este dispositivo. La duplicación de pantalla sigue sin estar disponible en Windows.",
    ios_not_ready: "Desbloquea el dispositivo y toca Confiar si se te pide. Debería aparecer aquí en unos segundos.",
    ios_windows_unsupported: "Apple no ofrece una API de captura de pantalla en Windows, por lo que la duplicación de iPhone y iPad solo está disponible en la versión para macOS de ScreenLink Studio.",
  },
  fr: {
    android_unauthorized: "Déverrouillez le téléphone et appuyez sur Autoriser à la demande « Autoriser le débogage USB ? ». Si elle n'apparaît pas, débranchez puis rebranchez le câble.",
    android_offline: "L'appareil ne répond pas à adb. Rebranchez le câble ou redémarrez l'appareil.",
    android_not_booted: "L'appareil n'a pas fini de démarrer. Démarrez-le normalement, puis rebranchez-le.",
    android_too_old: "La recopie d'écran nécessite Android 5.0 ou version ultérieure.",
    android_no_audio: "La transmission audio nécessite Android 11 ou version ultérieure. La vidéo et le contrôle fonctionnent normalement.",
    android_debugging_off: "Ce téléphone est connecté, mais ne répond pas à adb. Vérifiez que le débogage USB est activé dans les options pour les développeurs, puis rebranchez le câble.",
    ios_not_trusted: "Déverrouillez l'appareil et appuyez sur Se fier. Si rien n'apparaît, ouvrez le Finder (macOS) ou l'app Appareils Apple (Windows) avec l'appareil connecté.",
    ios_view_only: "Recopie en affichage seul. iOS ne permet pas à un ordinateur de contrôler l'appareil.",
    ios_no_driver: "Installez l'app Appareils Apple depuis le Microsoft Store pour identifier cet appareil. La recopie d'écran reste indisponible sous Windows.",
    ios_not_ready: "Déverrouillez l'appareil et appuyez sur Se fier si cela vous est demandé. Il devrait apparaître ici dans quelques secondes.",
    ios_windows_unsupported: "Apple ne fournit pas d'API de capture d'écran sous Windows : la recopie d'iPhone et d'iPad est disponible uniquement dans la version macOS de ScreenLink Studio.",
  },
};

export function compatText(c: Compatibility, language: string): string {
  return translations[language]?.[c.code] ?? c.message;
}
