import type { PluginMessageEvent, PluginUIEvent } from './model.js';

logos.ui.open('FEATHER ICONS PLUGIN', `?theme=${logos.theme}`, {
  width: 292,
  height: 540,
});

logos.ui.onMessage<PluginUIEvent>((message) => {
  if (message.type === 'insert-icon') {
    const { name, svg } = message.content;

    if (!svg || !name) {
      return;
    }

    const icon = logos.createShapeFromSvg(svg);
    if (icon) {
      icon.name = name;
      icon.x = logos.viewport.center.x;
      icon.y = logos.viewport.center.y;
    }
  }
});

logos.on('themechange', (theme) => {
  sendMessage({ type: 'theme', content: theme });
});

function sendMessage(message: PluginMessageEvent) {
  logos.ui.sendMessage(message);
}
