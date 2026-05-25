const fs = require('fs');

let css = fs.readFileSync('C:\\Users\\tjcri\\CerberusAI-Desktop\\src\\style.css', 'utf8');

css = css.replace(/--red-/g, '--purp-');
css = css.replace(/#dc2626/gi, '#7e22ce');
css = css.replace(/#ef4444/gi, '#a855f7');
css = css.replace(/#f87171/gi, '#c084fc');
css = css.replace(/220,\s*38,\s*38/g, '168, 85, 247');
// Update the specific color mentioned in subagent report: #2a1111 (reddish user bubble) to dark purple
css = css.replace(/#2a1111/gi, '#24143d'); 

fs.writeFileSync('C:\\Users\\tjcri\\CerberusAI-Desktop\\src\\style.css', css, 'utf8');
console.log('Colors replaced successfully');
