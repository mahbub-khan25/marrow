// Safari runs this in the page being shared, so Marrow gets the page as
// it's shown (signed in, expanded), and the part that was selected.
var GetPage = function () {};
GetPage.prototype = {
  run: function (args) {
    var sel = window.getSelection();
    var selection = "";
    if (sel && sel.rangeCount && !sel.isCollapsed) {
      var box = document.createElement("div");
      for (var i = 0; i < sel.rangeCount; i++) box.appendChild(sel.getRangeAt(i).cloneContents());
      selection = box.innerHTML;
    }
    args.completionFunction({
      url: document.URL,
      title: document.title,
      html: document.documentElement.outerHTML,
      selection: selection,
    });
  },
  finalize: function () {},
};
var ExtensionPreprocessingJS = new GetPage();
