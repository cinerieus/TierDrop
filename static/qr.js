// QR Join modal — uses qrcode-generator library
// Uses delegated event listener to work with hx-boost page swaps
(function(){
"use strict";

function showQrModal(nwid){
  // Remove any existing QR modal
  var old=document.getElementById("qr-modal");
  if(old) old.remove();

  var url="https://joinzt.com/addnetwork?nwid="+nwid+"&v=1";
  var qr=qrcode(0,"M");
  qr.addData(url);
  qr.make();

  // Build modal
  var backdrop=document.createElement("div");
  backdrop.id="qr-modal";
  backdrop.className="modal-backdrop";
  backdrop.addEventListener("click",function(e){if(e.target===backdrop)backdrop.remove();});

  var modal=document.createElement("div");
  modal.className="modal";
  modal.style.maxWidth="340px";
  modal.style.textAlign="center";

  var header=document.createElement("div");
  header.className="modal-header";
  var h3=document.createElement("h3");
  h3.textContent="Join Network";
  var closeBtn=document.createElement("button");
  closeBtn.className="modal-close";
  closeBtn.innerHTML="&times;";
  closeBtn.addEventListener("click",function(){backdrop.remove();});
  header.appendChild(h3);
  header.appendChild(closeBtn);

  var body=document.createElement("div");
  body.className="modal-body";
  body.style.display="flex";
  body.style.flexDirection="column";
  body.style.alignItems="center";
  body.style.gap="12px";

  var subtitle=document.createElement("div");
  subtitle.className="text-secondary";
  subtitle.style.fontSize="13px";
  subtitle.textContent="Scan with ZeroTier mobile app";

  // Render QR to canvas with proper quiet zone and crisp pixels
  var canvas=document.createElement("canvas");
  var count=qr.getModuleCount();
  var scale=8, quiet=4;
  var total=(count+quiet*2)*scale;
  canvas.width=total;
  canvas.height=total;
  canvas.style.width=total+"px";
  canvas.style.height=total+"px";
  canvas.style.imageRendering="pixelated";
  canvas.style.borderRadius="6px";
  var ctx=canvas.getContext("2d");
  ctx.fillStyle="#ffffff";
  ctx.fillRect(0,0,total,total);
  ctx.fillStyle="#000000";
  for(var r=0;r<count;r++)
    for(var c=0;c<count;c++)
      if(qr.isDark(r,c)) ctx.fillRect((c+quiet)*scale,(r+quiet)*scale,scale,scale);

  var nwidEl=document.createElement("div");
  nwidEl.className="mono text-secondary";
  nwidEl.style.fontSize="12px";
  nwidEl.textContent=nwid;

  body.appendChild(subtitle);
  body.appendChild(canvas);
  body.appendChild(nwidEl);
  modal.appendChild(header);
  modal.appendChild(body);
  backdrop.appendChild(modal);
  document.body.appendChild(backdrop);
}

// Delegated click handler — survives hx-boost body swaps
document.addEventListener("click",function(e){
  var btn=e.target.closest(".qr-join-btn");
  if(btn){
    e.preventDefault();
    showQrModal(btn.getAttribute("data-nwid"));
  }
});
})();
