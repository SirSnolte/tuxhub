const $ = (id) => document.getElementById(id);

tuxly.on("status", (s) => {
  $("count").textContent = s.count ?? 0;
  $("text").textContent = s.text ?? "";
});
$("btn").onclick = () => tuxly.action("count").catch((e) => alert(e.message));
$("save").onclick = () => tuxly.saveSettings({ greeting: $("greeting").value });
tuxly.getSettings().then((s) => ($("greeting").value = s.greeting));
tuxly.call("echo", { ping: "pong" }).then((r) => ($("echo").textContent = "Echo from the process: " + JSON.stringify(r)));
