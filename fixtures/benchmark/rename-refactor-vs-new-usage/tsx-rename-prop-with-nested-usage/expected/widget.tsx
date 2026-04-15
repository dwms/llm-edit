function greet(accountName) {
  return `hello ${accountName}`;
}

function farewell(accountName) {
  return `bye ${accountName}`;
}

function salute(accountName, level) {
  const header = `--- accountName=${accountName} level=${level} ---`;
  return header + greet(accountName) + farewell(accountName);
}
