let cy = cytoscape({
    container: document.getElementById("cy"),
    elements: [
        // Define your network graph elements here
        // { data: { id: "node1" } },
        // { data: { id: "node2" } },
        // { data: { id: "node3" } },
        // { data: { id: "node4" } },
        // { data: { id: "edge1", source: "node1", target: "node2", label: "Connection" } },
        // { data: { id: "edge2", source: "node1", target: "node2", label: "Connection2" } },
    ],
    layout: {
        name: "circle",
    },
    style: [
        // Define your network graph styles here
        {
            selector: "node",
            style: {
                "background-color": "#666",
                label: "data(id)",
            },
        },
        {
            selector: "edge",
            style: {
                width: 3,
                "curve-style": "bezier",
                "control-point-step-size": 40,
                "line-color": "#ccc",
                "target-arrow-color": "#ccc",
                "target-arrow-shape": "triangle",
                label: "data(label)",
            },
        },
    ],
    layout: {
        name: "grid",
    },
});

function edgeKey(from, to, uuid, is_outgoing) {
    if (is_outgoing) {
        return "e" + from + "-" + to + "-" + uuid;
    } else {
        return "e" + to + "-" + from + "-" + uuid;
    }
}

let edges = {};

function resetGraph(nodes) {
    cy.startBatch();
    cy.removeData();
    nodes.map((node) => {
        cy.add({ data: { id: node.id } });
        node.connections.map((connection) => {
            let edge = edgeKey(node.id, connection.dest, connection.uuid, connection.outgoing);
            if (edges[edge] === undefined) {
                edges[edge] = {
                    id: edge,
                    source: connection.outgoing ? node.id : connection.dest,
                    target: connection.outgoing ? connection.dest : node.id,
                    nodes: {
                        [node.id]: {
                            remote: connection.remote,
                            rtt_ms: connection.rtt_ms,
                        },
                    },
                };
            } else {
                edges[edge].nodes[node.id] = {
                    remote: connection.remote,
                    rtt_ms: connection.rtt_ms,
                };
            }
        });
    });
    Object.values(edges).map((edge) => {
        let label = Object.values(edge.nodes).map((n) => n.remote + " / " + n.rtt_ms).join(", ");
        cy.add({
            data: {
                id: edge.id,
                source: edge.source,
                target: edge.target,
                label,
            },
        });
    });
    cy.endBatch();
    cy.layout({ name: "circle" }).run();
}

function updateNode(node) {
    let added = false;
    if (cy.$id(node.id).length == 0) {
        added = true;
        cy.add({ data: { id: node.id } });
    }
    cy.data({ elements: { data: { id: node.id } } });
    node.connections.map((connection) => {
        if (cy.$id(connection.dest).length == 0) {
            added = true;
            cy.add({ data: { id: connection.dest } });
        }

        let edge = edgeKey(node.id, connection.dest, connection.uuid, connection.outgoing);
        if (edges[edge] === undefined) {
            edges[edge] = {
                id: edge,
                source: connection.outgoing ? node.id : connection.dest,
                target: connection.outgoing ? connection.dest : node.id,
                nodes: {
                    [node.id]: {
                        remote: connection.remote,
                        rtt_ms: connection.rtt_ms,
                    },
                },
            };
        } else {
            edges[edge].nodes[node.id] = {
                remote: connection.remote,
                rtt_ms: connection.rtt_ms,
            };
        }

        let label = Object.values(edges[edge].nodes).map((n) => n.remote + " / " + n.rtt_ms).join(", ");
        let element = cy.$id(edge);
        if (element.length > 0) {
            element.data('label', label)
        } else {
            cy.add({
                data: {
                    id: edge,
                    source: edges[edge].source,
                    target: edges[edge].target,
                    label,
                },
            });
        }
    });
    if(added) {
        cy.layout({ name: "circle" }).run();
    }
}

function deleteNode(nodeId) {
    cy.remove(cy.$id(nodeId));
}

function connect(path) {
    //create websocket url for current page
    let url =
        (window.location.protocol === "https:" ? "wss" : "ws") +
        "://" +
        window.location.host +
        path;
    console.log(url);
    let ws = new WebSocket(url);
    ws.onmessage = function (event) {
        let data = JSON.parse(event.data);
        switch (data.type) {
            case "Snapshot": {
                resetGraph(data.data);
                break;
            }
            case "Update": {
                updateNode(data.data);
                break;
            }
            case "Delete": {
                deleteNode(data.data);
                break;
            }
            default:
                console.error("Unknown message type: " + data.type);
        }
    };
}

connect("/ws");
