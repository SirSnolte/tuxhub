const $ = (id) => document.getElementById(id);

tuxhub.on("status", (s) => {
  $("count").textContent = s.count ?? 0;
  $("text").textContent = s.text ?? "";
});
$("btn").onclick = () => tuxhub.action("count").catch((e) => alert(e.message));
$("save").onclick = () => tuxhub.saveSettings({ greeting: $("greeting").value });
tuxhub.getSettings().then((s) => ($("greeting").value = s.greeting));
tuxhub.call("echo", { ping: "pong" }).then((r) => ($("echo").textContent = "Echo from the process: " + JSON.stringify(r)));
