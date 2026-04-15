function greet(userName) {
  return `hello ${userName}`;
}

function farewell(userName) {
  return `bye ${userName}`;
}

function salute(userName, level) {
  const header = `--- userName=${userName} level=${level} ---`;
  return header + greet(userName) + farewell(userName);
}
