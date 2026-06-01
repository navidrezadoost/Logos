export default function () {
  const rectangle = logos.createRectangle();

  rectangle?.setPluginData('testData', 'test');
  return rectangle?.getPluginData('testData');
}
