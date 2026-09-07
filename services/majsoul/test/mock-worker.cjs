process.on("message", ({ uuid }) => {
  if (uuid === "crash") process.exit(1);
  else if (uuid !== "timeout") process.send({ log: { uuid } });
});
