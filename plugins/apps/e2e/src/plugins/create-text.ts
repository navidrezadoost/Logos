export default function () {
  function createText(): void {
    const text = logos.createText('Hello World!');

    if (text) {
      text.x = logos.viewport.center.x;
      text.y = logos.viewport.center.y;
      text.growType = 'auto-width';
      text.textTransform = 'uppercase';
      text.textDecoration = 'underline';
      text.fontId = 'gfont-work-sans';
      text.fontStyle = 'italic';
      text.fontSize = '20';
      text.fontWeight = '500';

      const textRange = text.getRange(0, 5);
      textRange.fontSize = '40';
      textRange.fills = [{ fillColor: '#ff6fe0', fillOpacity: 1 }];
    }
  }

  createText();
}
